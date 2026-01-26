use std::fs;
use std::path::Path;
use std::sync::Mutex;

use rand::seq::SliceRandom;
use rand::{rngs::StdRng, SeedableRng};
use serde_json::Value;
use tonic::{Request, Response, Status};

use joker_env::proto::joker_env_server::{JokerEnv, JokerEnvServer};
use joker_env::proto::{
    Action, EnvInfo, GetSpecRequest, GetSpecResponse, Observation, ResetRequest, ResetResponse,
    StepRequest, StepResponse, Tensor, TensorSpec,
};

const HAND_SIZE: usize = 5;
const JOKER_SLOTS: usize = 5;
const JOKER_START_COUNT: usize = 0;
const HANDS_PER_ROUND: i32 = 4;
const DISCARDS_PER_ROUND: i32 = 3;
const SCALAR_COUNT: usize = 8;
const SELECTION_FEATURES: usize = HAND_SIZE;
const CARD_FEATURES: usize = 17; // 13 rank one-hot + 4 suit one-hot
const HAND_FEATURES: usize = HAND_SIZE * CARD_FEATURES;
const HAND_TYPE_COUNT: usize = 10;
const DECK_FEATURES: usize = 52;
const JOKER_FEATURES: usize = JOKER_SLOTS * 2; // id + enabled
const OBS_SIZE: i32 = (SCALAR_COUNT
    + SELECTION_FEATURES
    + HAND_FEATURES
    + HAND_TYPE_COUNT
    + DECK_FEATURES
    + JOKER_FEATURES) as i32;
const ACTION_SPACE: i32 = 1 << HAND_SIZE;
const ACTION_MASK_SIZE: i32 = 3 + (HAND_SIZE as i32 * 2);
const ACTION_TYPE_PLAY: i32 = 0;
const ACTION_TYPE_DISCARD: i32 = 1;
const ACTION_TYPE_SELECT: i32 = 2;
const MAX_STEPS: i32 = 20;
const BLIND_TARGET: i64 = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Card {
    rank: u8, // 1..=13 (Ace = 1)
    suit: u8, // 0..=3
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HandId {
    HighCard,
    Pair,
    TwoPair,
    ThreeKind,
    Straight,
    Flush,
    FullHouse,
    FourKind,
    StraightFlush,
    RoyalFlush,
}

struct HandScore {
    base_chips: i64,
    base_mult: i64,
    id: HandId,
}

struct JokerSlot {
    id: i32,
    enabled: bool,
    type_key: String,
}

struct EnvState {
    rng: StdRng,
    deck: Vec<Card>,
    hand: Vec<Card>,
    jokers: Vec<JokerSlot>,
    joker_slot_limit: usize,
    selected_mask: u32,
    hands_left: i32,
    discards_left: i32,
    episode_step: i32,
    chips: i64,
    mult: i64,
    blind_target: i64,
}

impl EnvState {
    fn new(seed: u64, joker_catalog_len: usize, joker_types: &[String]) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let (hand, deck) = deal_new_hand(&mut rng);
        let joker_slot_limit = JOKER_SLOTS;
        let jokers = seed_jokers(
            &mut rng,
            joker_catalog_len,
            joker_slot_limit,
            JOKER_START_COUNT,
            joker_types,
        );
        Self {
            rng,
            deck,
            hand,
            jokers,
            joker_slot_limit,
            selected_mask: ((1 << HAND_SIZE) - 1) as u32,
            hands_left: HANDS_PER_ROUND,
            discards_left: DISCARDS_PER_ROUND,
            episode_step: 0,
            chips: 0,
            mult: 1,
            blind_target: BLIND_TARGET,
        }
    }
}

struct EnvService {
    state: Mutex<EnvState>,
    joker_catalog_len: usize,
    joker_type_keys: Vec<String>,
}

impl Default for EnvService {
    fn default() -> Self {
        let joker_catalog_len = load_joker_catalog_len();
        let joker_type_keys = load_joker_type_keys();
        Self {
            state: Mutex::new(EnvState::new(0, joker_catalog_len, &joker_type_keys)),
            joker_catalog_len,
            joker_type_keys,
        }
    }
}

#[tonic::async_trait]
impl JokerEnv for EnvService {
    async fn reset(
        &self,
        request: Request<ResetRequest>,
    ) -> Result<Response<ResetResponse>, Status> {
        let seed = request.into_inner().seed;
        let mut state = self
            .state
            .lock()
            .map_err(|_| Status::internal("lock error"))?;
        *state = EnvState::new(seed, self.joker_catalog_len, &self.joker_type_keys);

        let observation = Observation {
            features: Some(observation_from_state(&state)),
            action_mask: Some(action_mask_from_state(&state, false)),
        };

        let info = EnvInfo {
            episode_step: state.episode_step,
            chips: state.chips,
            mult: state.mult,
            blind_target: state.blind_target,
        };

        Ok(Response::new(ResetResponse {
            observation: Some(observation),
            info: Some(info),
        }))
    }

    async fn step(&self, request: Request<StepRequest>) -> Result<Response<StepResponse>, Status> {
        let StepRequest { action } = request.into_inner();
        let action = action.unwrap_or(Action {
            action_id: 0,
            params: vec![],
            action_type: ACTION_TYPE_PLAY,
        });

        let mask = (action.action_id as u32) & ((1 << HAND_SIZE) - 1);
        let action_type = match action.action_type {
            ACTION_TYPE_DISCARD => ACTION_TYPE_DISCARD,
            ACTION_TYPE_SELECT => ACTION_TYPE_SELECT,
            _ => ACTION_TYPE_PLAY,
        };

        let mut state = self
            .state
            .lock()
            .map_err(|_| Status::internal("lock error"))?;

        if action_type == ACTION_TYPE_SELECT {
            state.selected_mask = if mask == 0 { 1 } else { mask };

            let observation = Observation {
                features: Some(observation_from_state(&state)),
                action_mask: Some(action_mask_from_state(&state, false)),
            };

            let info = EnvInfo {
                episode_step: state.episode_step,
                chips: state.chips,
                mult: state.mult,
                blind_target: state.blind_target,
            };

            return Ok(Response::new(StepResponse {
                observation: Some(observation),
                reward: 0.0,
                done: false,
                info: Some(info),
            }));
        }

        if action_type == ACTION_TYPE_DISCARD
            && (action.action_id < 0 || action.action_id >= ACTION_SPACE)
        {
            return Err(Status::invalid_argument("action_id out of range"));
        }

        let mut reward = 0.0;
        let mut win = false;

        if action_type == ACTION_TYPE_DISCARD {
            if state.discards_left > 0 {
                let mut hand = std::mem::take(&mut state.hand);
                let mut deck = std::mem::take(&mut state.deck);
                apply_discard(&mut hand, &mut deck, mask);
                state.hand = hand;
                state.deck = deck;
                state.discards_left -= 1;
            }
        } else if state.hands_left > 0 {
            let selected_hand = build_selected_hand(&state.hand, state.selected_mask);
            let hand_score = score_hand(&selected_hand);
            let bonus = compute_joker_bonus(&state.jokers);
            let base_mult = hand_score.base_mult + bonus.add_mult;
            let effective_mult = ((base_mult as f32) * bonus.mul_mult).max(1.0) as i64;
            let hand_score_value = hand_score.base_chips * effective_mult;
            let prev_chips = state.chips;
            state.chips += hand_score_value + bonus.chip_bonus;
            state.mult = effective_mult;
            let chip_gain = state.chips - prev_chips;

            // 中間步驟：小獎勵引導學習
            reward = shaping_reward(chip_gain, state.blind_target);
            state.hands_left -= 1;
            state.discards_left = DISCARDS_PER_ROUND;
            state.selected_mask = ((1 << HAND_SIZE) - 1) as u32;

            win = state.chips >= state.blind_target;

            if state.hands_left > 0 && !win {
                let (hand, deck) = deal_new_hand(&mut state.rng);
                state.hand = hand;
                state.deck = deck;
            }
        }

        state.episode_step += 1;

        let done = win || state.hands_left <= 0 || state.episode_step >= MAX_STEPS;

        // 回合結束時給大獎勵/懲罰
        if done {
            reward += terminal_reward(state.chips, state.blind_target, win);
        }

        let observation = Observation {
            features: Some(observation_from_state(&state)),
            action_mask: Some(action_mask_from_state(&state, done)),
        };

        let info = EnvInfo {
            episode_step: state.episode_step,
            chips: state.chips,
            mult: state.mult,
            blind_target: state.blind_target,
        };

        Ok(Response::new(StepResponse {
            observation: Some(observation),
            reward,
            done,
            info: Some(info),
        }))
    }

    async fn get_spec(
        &self,
        _request: Request<GetSpecRequest>,
    ) -> Result<Response<GetSpecResponse>, Status> {
        let observation = TensorSpec {
            shape: vec![OBS_SIZE],
            dtype: "f32".to_string(),
        };

        let action_mask = TensorSpec {
            shape: vec![ACTION_MASK_SIZE],
            dtype: "f32".to_string(),
        };

        Ok(Response::new(GetSpecResponse {
            observation: Some(observation),
            action_mask: Some(action_mask),
            action_space: ACTION_MASK_SIZE,
        }))
    }
}

fn standard_deck() -> Vec<Card> {
    let mut deck = Vec::with_capacity(52);
    for suit in 0..4 {
        for rank in 1..=13 {
            deck.push(Card { rank, suit });
        }
    }
    deck
}

fn deal_new_hand(rng: &mut StdRng) -> (Vec<Card>, Vec<Card>) {
    let mut deck = standard_deck();
    deck.shuffle(rng);
    let hand = deck.drain(0..HAND_SIZE).collect::<Vec<_>>();
    (hand, deck)
}

fn apply_discard(hand: &mut [Card], deck: &mut Vec<Card>, discard_mask: u32) {
    for idx in 0..HAND_SIZE {
        if (discard_mask >> idx) & 1 == 1 {
            if let Some(card) = deck.pop() {
                hand[idx] = card;
            }
        }
    }
}

fn build_selected_hand(hand: &[Card], mask: u32) -> Vec<Card> {
    let mut selected = Vec::with_capacity(HAND_SIZE);
    let mut used = [false; HAND_SIZE];
    for idx in 0..HAND_SIZE {
        if ((mask >> idx) & 1) == 1 {
            selected.push(hand[idx]);
            used[idx] = true;
        }
    }

    for idx in 0..HAND_SIZE {
        if selected.len() >= HAND_SIZE {
            break;
        }
        if !used[idx] {
            selected.push(hand[idx]);
        }
    }

    while selected.len() < HAND_SIZE {
        selected.push(hand[0]);
    }

    selected
}

struct JokerBonus {
    chip_bonus: i64,
    add_mult: i64,
    mul_mult: f32,
}

fn compute_joker_bonus(jokers: &[JokerSlot]) -> JokerBonus {
    let mut bonus = JokerBonus {
        chip_bonus: 0,
        add_mult: 0,
        mul_mult: 1.0,
    };

    for joker in jokers.iter().filter(|j| j.enabled) {
        match joker.type_key.as_str() {
            "+c" => bonus.chip_bonus += 5,
            "!!" => bonus.chip_bonus += 15,
            "+$" => bonus.chip_bonus += 10,
            "+m" => bonus.add_mult += 1,
            "++" => bonus.add_mult += 2,
            "Xm" => bonus.mul_mult *= 1.2,
            "..." => bonus.chip_bonus += 3,
            _ => bonus.chip_bonus += 2,
        }
    }

    bonus
}

fn observation_from_state(state: &EnvState) -> Tensor {
    let mut data = Vec::with_capacity(OBS_SIZE as usize);

    data.push(normalize(state.chips as f32, state.blind_target as f32));
    data.push(normalize(state.mult as f32, 10.0));
    data.push(normalize(state.blind_target as f32, BLIND_TARGET as f32));
    data.push(normalize(state.episode_step as f32, MAX_STEPS as f32));
    data.push(normalize(state.hands_left as f32, HANDS_PER_ROUND as f32));
    data.push(normalize(
        state.discards_left as f32,
        DISCARDS_PER_ROUND as f32,
    ));
    data.push(state.deck.len() as f32 / 52.0);
    data.push(normalize(
        state.jokers.len() as f32,
        state.joker_slot_limit as f32,
    ));

    for idx in 0..HAND_SIZE {
        let selected = ((state.selected_mask >> idx) & 1) == 1;
        data.push(if selected { 1.0 } else { 0.0 });
    }

    for idx in 0..HAND_SIZE {
        if let Some(card) = state.hand.get(idx) {
            append_card_features(&mut data, *card);
        } else {
            data.extend(std::iter::repeat(0.0).take(CARD_FEATURES));
        }
    }

    let hand_id = score_hand(&state.hand).id;
    append_hand_type_features(&mut data, hand_id);

    let mut deck_counts = [0.0f32; DECK_FEATURES];
    for card in &state.deck {
        let index = card_index(*card);
        deck_counts[index] += 1.0;
    }
    data.extend(deck_counts);

    for slot in 0..JOKER_SLOTS {
        if let Some(joker) = state.jokers.get(slot) {
            data.push(joker.id as f32);
            data.push(if joker.enabled { 1.0 } else { 0.0 });
        } else {
            data.push(0.0);
            data.push(0.0);
        }
    }

    if data.len() < OBS_SIZE as usize {
        data.extend(std::iter::repeat(0.0).take(OBS_SIZE as usize - data.len()));
    }

    Tensor {
        data,
        shape: vec![OBS_SIZE],
    }
}

fn action_mask_from_state(state: &EnvState, done: bool) -> Tensor {
    let mut data = vec![0.0; ACTION_MASK_SIZE as usize];
    if done {
        return Tensor {
            data,
            shape: vec![ACTION_MASK_SIZE],
        };
    }

    let play_allowed = state.hands_left > 0;
    let discard_allowed = state.discards_left > 0;
    let select_allowed = state.hands_left > 0;

    data[0] = if play_allowed { 1.0 } else { 0.0 };
    data[1] = if discard_allowed { 1.0 } else { 0.0 };
    data[2] = if select_allowed { 1.0 } else { 0.0 };

    let mut offset = 3;
    for idx in 0..HAND_SIZE {
        data[offset] = if discard_allowed { 1.0 } else { 0.0 };
        let selected = ((state.selected_mask >> idx) & 1) == 1;
        data[offset + 1] = if selected { 1.0 } else { 0.0 };
        offset += 2;
    }

    Tensor {
        data,
        shape: vec![ACTION_MASK_SIZE],
    }
}

fn load_joker_catalog_len() -> usize {
    let path = Path::new("../data/joker-ids.json");
    let Ok(data) = fs::read_to_string(path) else {
        return 0;
    };
    let Ok(ids) = serde_json::from_str::<Vec<String>>(&data) else {
        return 0;
    };
    ids.len()
}

fn load_joker_type_keys() -> Vec<String> {
    let path = Path::new("../data/jokers-meta.json");
    let Ok(data) = fs::read_to_string(path) else {
        return vec!["+c".to_string()];
    };
    let Ok(parsed) = serde_json::from_str::<Value>(&data) else {
        return vec!["+c".to_string()];
    };

    let mut keys = Vec::new();
    if let Some(types) = parsed.get("types").and_then(|t| t.as_object()) {
        keys.extend(types.keys().cloned());
    }
    if keys.is_empty() {
        keys.push("+c".to_string());
    }
    keys
}

fn seed_jokers(
    rng: &mut StdRng,
    catalog_len: usize,
    slot_limit: usize,
    count: usize,
    type_keys: &[String],
) -> Vec<JokerSlot> {
    if catalog_len == 0 || slot_limit == 0 || count == 0 {
        return Vec::new();
    }

    let count = count.min(slot_limit).min(catalog_len);
    let mut ids = (1..=catalog_len as i32).collect::<Vec<i32>>();
    ids.shuffle(rng);
    ids.truncate(count);

    ids.into_iter()
        .map(|id| JokerSlot {
            id,
            enabled: true,
            type_key: type_keys
                .choose(rng)
                .cloned()
                .unwrap_or_else(|| "+c".to_string()),
        })
        .collect()
}

fn normalize(value: f32, max_value: f32) -> f32 {
    if max_value <= 0.0 {
        0.0
    } else {
        value / max_value
    }
}

/// 中間步驟的引導獎勵（shaping reward）
/// 根據這一手得到的分數佔目標的比例給小獎勵
fn shaping_reward(chip_gain: i64, blind_target: i64) -> f32 {
    if blind_target <= 0 || chip_gain <= 0 {
        return 0.0;
    }
    // 每手牌根據貢獻比例給 0~2 分
    let ratio = (chip_gain as f32 / blind_target as f32).min(1.0);
    ratio * 2.0
}

/// 回合結束時的獎勵
/// - 失敗: -10
/// - 過關: +50
/// - 超過目標: +50 + 額外獎勵（按超出比例，上限 +50）
fn terminal_reward(chips: i64, blind_target: i64, is_win: bool) -> f32 {
    if !is_win {
        return -10.0; // 失敗懲罰
    }

    // 過關基礎獎勵
    let base_reward = 50.0;

    // 超過目標的額外獎勵
    let overflow = chips - blind_target;
    if overflow > 0 && blind_target > 0 {
        let bonus = (overflow as f32 / blind_target as f32) * 20.0;
        base_reward + bonus.min(50.0) // 上限 100
    } else {
        base_reward
    }
}

// 保留舊函數供測試使用
#[allow(dead_code)]
fn reward_from_gain(chip_gain: i64, blind_target: i64) -> f32 {
    if blind_target <= 0 || chip_gain <= 0 {
        return 0.0;
    }

    let percentage = (chip_gain as f32 / blind_target as f32) * 100.0;
    if percentage >= 100.0 {
        20.0
    } else if percentage >= 75.0 {
        10.0
    } else if percentage >= 40.0 {
        4.0
    } else if percentage >= 20.0 {
        1.0
    } else {
        0.5
    }
}

fn append_card_features(buffer: &mut Vec<f32>, card: Card) {
    for idx in 0..13 {
        buffer.push(if idx == (card.rank - 1) as usize {
            1.0
        } else {
            0.0
        });
    }
    for idx in 0..4 {
        buffer.push(if idx == card.suit as usize { 1.0 } else { 0.0 });
    }
}

fn append_hand_type_features(buffer: &mut Vec<f32>, hand_id: HandId) {
    let index = hand_type_index(hand_id);
    for idx in 0..HAND_TYPE_COUNT {
        buffer.push(if idx == index { 1.0 } else { 0.0 });
    }
}

fn hand_type_index(hand_id: HandId) -> usize {
    match hand_id {
        HandId::HighCard => 0,
        HandId::Pair => 1,
        HandId::TwoPair => 2,
        HandId::ThreeKind => 3,
        HandId::Straight => 4,
        HandId::Flush => 5,
        HandId::FullHouse => 6,
        HandId::FourKind => 7,
        HandId::StraightFlush => 8,
        HandId::RoyalFlush => 9,
    }
}

fn card_index(card: Card) -> usize {
    (card.suit as usize * 13) + (card.rank as usize - 1)
}

fn score_hand(hand: &[Card]) -> HandScore {
    let mut rank_counts = [0u8; 13];
    let mut suit_counts = [0u8; 4];
    let mut ranks = Vec::with_capacity(HAND_SIZE);

    for card in hand.iter().take(HAND_SIZE) {
        rank_counts[(card.rank - 1) as usize] += 1;
        suit_counts[card.suit as usize] += 1;
        ranks.push(card.rank);
    }

    let is_flush = suit_counts.iter().any(|&count| count == HAND_SIZE as u8);
    let is_straight = is_straight(&ranks);
    let is_royal = is_flush && is_straight && is_royal_ranks(&ranks);

    let mut count_values = rank_counts
        .iter()
        .cloned()
        .filter(|&count| count > 0)
        .collect::<Vec<u8>>();
    count_values.sort_unstable_by(|a, b| b.cmp(a));

    let id = if is_flush && is_straight {
        if is_royal {
            HandId::RoyalFlush
        } else {
            HandId::StraightFlush
        }
    } else if count_values[0] == 4 {
        HandId::FourKind
    } else if count_values[0] == 3 && count_values[1] == 2 {
        HandId::FullHouse
    } else if is_flush {
        HandId::Flush
    } else if is_straight {
        HandId::Straight
    } else if count_values[0] == 3 {
        HandId::ThreeKind
    } else if count_values[0] == 2 && count_values[1] == 2 {
        HandId::TwoPair
    } else if count_values[0] == 2 {
        HandId::Pair
    } else {
        HandId::HighCard
    };

    let (base_chips, base_mult) = match id {
        HandId::HighCard => (5, 1),
        HandId::Pair => (10, 2),
        HandId::TwoPair => (20, 2),
        HandId::ThreeKind => (30, 3),
        HandId::Straight => (30, 4),
        HandId::Flush => (35, 4),
        HandId::FullHouse => (40, 4),
        HandId::FourKind => (60, 7),
        HandId::StraightFlush | HandId::RoyalFlush => (100, 8),
    };

    HandScore {
        base_chips,
        base_mult,
        id,
    }
}

fn is_straight(ranks: &[u8]) -> bool {
    let mut uniq = ranks.to_vec();
    uniq.sort_unstable();
    uniq.dedup();
    if uniq.len() != HAND_SIZE {
        return false;
    }

    if uniq == vec![1, 2, 3, 4, 5] {
        return true;
    }

    let mut high = uniq
        .iter()
        .map(|&rank| if rank == 1 { 14 } else { rank })
        .collect::<Vec<u8>>();
    high.sort_unstable();

    for idx in 1..high.len() {
        if high[idx] != high[0] + idx as u8 {
            return false;
        }
    }

    true
}

fn is_royal_ranks(ranks: &[u8]) -> bool {
    let mut high = ranks
        .iter()
        .map(|&rank| if rank == 1 { 14 } else { rank })
        .collect::<Vec<u8>>();
    high.sort_unstable();
    high == vec![10, 11, 12, 13, 14]
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const R_LATRO_HAND_REF: &str = "references/RLatro/RLatro.Test/CoreRules/HandEvaluationTest.cs";
    const JOKER_TYPE_KEYS: [&str; 7] = ["+c", "!!", "+$", "+m", "++", "Xm", "..."];
    const JOKER_EFFECTS: [(&str, i64, i64, f32); 7] = [
        ("+c", 5, 0, 1.0),
        ("!!", 15, 0, 1.0),
        ("+$", 10, 0, 1.0),
        ("+m", 0, 1, 1.0),
        ("++", 0, 2, 1.0),
        ("Xm", 0, 0, 1.2),
        ("...", 3, 0, 1.0),
    ];

    fn card(rank: u8, suit: u8) -> Card {
        Card { rank, suit }
    }

    fn joker_slot(id: i32, type_key: &'static str, enabled: bool) -> JokerSlot {
        JokerSlot {
            id,
            enabled,
            type_key: type_key.to_string(),
        }
    }

    fn full_house_three_kings() -> [Card; HAND_SIZE] {
        [
            card(13, 0),
            card(13, 1),
            card(13, 2),
            card(1, 0),
            card(1, 1),
        ]
    }

    fn full_house_with_variation() -> [Card; HAND_SIZE] {
        [
            card(9, 0),
            card(9, 1),
            card(12, 0),
            card(12, 1),
            card(12, 2),
        ]
    }

    fn full_house_holo_poly() -> [Card; HAND_SIZE] {
        [
            card(1, 0),
            card(13, 1),
            card(1, 0),
            card(13, 1),
            card(13, 2),
        ]
    }

    fn full_house_with_steel_in_hand() -> [Card; HAND_SIZE] {
        [card(1, 0), card(1, 1), card(13, 2), card(13, 3), card(1, 2)]
    }

    #[test]
    fn full_house_matches_rlatro_reference() {
        let hand = full_house_with_variation();
        let hand_score = score_hand(&hand);
        assert_eq!(
            hand_score.id,
            HandId::FullHouse,
            "Score should match the reference data in {}",
            R_LATRO_HAND_REF
        );
        assert_eq!(hand_score.base_chips, 40);
        assert_eq!(hand_score.base_mult, 4);
    }

    #[test]
    fn steel_full_house_still_scores_full_house() {
        let hand = full_house_with_steel_in_hand();
        let hand_score = score_hand(&hand);
        assert_eq!(
            hand_score.id,
            HandId::FullHouse,
            "Steel-enhanced cards should still resolve to the same poker rank"
        );
        assert_eq!(hand_score.base_chips, 40);
        assert_eq!(hand_score.base_mult, 4);
    }

    #[test]
    fn holo_poly_full_house_matches_reference() {
        let hand = full_house_holo_poly();
        let hand_score = score_hand(&hand);
        assert_eq!(
            hand_score.id,
            HandId::FullHouse,
            "Holo/Poly edition cards should still produce the reference hand rank"
        );
        assert_eq!(hand_score.base_chips, 40);
        assert_eq!(hand_score.base_mult, 4);
    }

    #[test]
    fn joker_bonus_compounds_correctly() {
        let hand = full_house_three_kings();
        let hand_score = score_hand(&hand);
        let jokers = vec![
            joker_slot(1, "+c", true),
            joker_slot(2, "+m", true),
            joker_slot(3, "Xm", true),
        ];
        let bonus = compute_joker_bonus(&jokers);

        assert_eq!(bonus.chip_bonus, 5);
        assert_eq!(bonus.add_mult, 1);
        assert!((bonus.mul_mult - 1.2).abs() < f32::EPSILON);

        let effective_mult =
            ((hand_score.base_mult + bonus.add_mult) as f32 * bonus.mul_mult).max(1.0) as i64;
        let total_gain = hand_score.base_chips * effective_mult + bonus.chip_bonus;

        // Expectation derived from Balatro-style bonuses (FullHouse + Jokers).
        assert_eq!(total_gain, 245);
        assert_eq!(reward_from_gain(total_gain, BLIND_TARGET), 10.0);
    }

    #[test]
    fn rare_jokers_push_score_higher() {
        let hand = full_house_with_variation();
        let jokers = vec![
            joker_slot(1, "Xm", true),
            joker_slot(2, "++", true),
            joker_slot(3, "+$", true),
        ];
        let bonus = compute_joker_bonus(&jokers);
        assert_eq!(bonus.chip_bonus, 10);
        assert_eq!(bonus.add_mult, 2);
        assert!((bonus.mul_mult - 1.2).abs() < f32::EPSILON);

        let hand_score = score_hand(&hand);
        let effective_mult =
            ((hand_score.base_mult + bonus.add_mult) as f32 * bonus.mul_mult).max(1.0) as i64;
        let total_gain = hand_score.base_chips * effective_mult + bonus.chip_bonus;

        assert!(total_gain > hand_score.base_chips);
        assert!(total_gain > hand_score.base_chips * hand_score.base_mult);
    }

    proptest! {
        #[test]
        fn score_hand_has_positive_base_values(
            ranks in prop::collection::vec(1u8..=13, HAND_SIZE),
            suits in prop::collection::vec(0u8..4, HAND_SIZE),
        ) {
            let hand: Vec<Card> = ranks
                .into_iter()
                .zip(suits.into_iter())
                .map(|(rank, suit)| card(rank, suit))
                .collect();
            let score = score_hand(&hand);
            prop_assert!(score.base_chips >= 5);
            prop_assert!(score.base_mult >= 1);
        }

        #[test]
        fn joker_bonus_never_negative(
            joker_specs in prop::collection::vec(
                (prop::sample::select(JOKER_TYPE_KEYS.as_slice()), any::<bool>()),
                0..=JOKER_SLOTS
            )
        ) {
            let jokers: Vec<JokerSlot> = joker_specs
                .into_iter()
                .enumerate()
                .map(|(idx, (key, enabled)): (usize, (&'static str, bool))| JokerSlot {
                    id: idx as i32 + 1,
                    enabled,
                    type_key: key.to_string(),
                })
                .collect();

            let bonus = compute_joker_bonus(&jokers);
            prop_assert!(bonus.chip_bonus >= 0);
            prop_assert!(bonus.add_mult >= 0);
            prop_assert!(bonus.mul_mult >= 1.0);
        }

        #[test]
        fn build_selected_hand_always_full(mask in 0u32..(1 << HAND_SIZE)) {
            let hand = (0..HAND_SIZE)
                .map(|idx| card(idx as u8 % 13 + 1, idx as u8 % 4))
                .collect::<Vec<_>>();
            let selected = build_selected_hand(&hand, mask);
            prop_assert_eq!(selected.len(), HAND_SIZE);
            for card in selected {
                prop_assert!(hand.contains(&card));
            }
        }
    }

    #[test]
    fn joker_effect_table_matches_metadata() {
        let base_hand = full_house_three_kings();

        for (type_key, chip_delta, add_mult, mult_scale) in JOKER_EFFECTS {
            let jokers = vec![joker_slot(1, type_key, true)];
            let bonus = compute_joker_bonus(&jokers);

            assert_eq!(
                bonus.chip_bonus, chip_delta,
                "RLatro metadata says {} should yield {} chips",
                type_key, chip_delta
            );
            assert_eq!(
                bonus.add_mult, add_mult,
                "{} should add {} to mult",
                type_key, add_mult
            );
            assert!(
                (bonus.mul_mult - mult_scale).abs() < f32::EPSILON,
                "{} should multiply by {} according to RLatro-inspired logic",
                type_key,
                mult_scale
            );

            let hand_score = score_hand(&base_hand);
            let effective_mult =
                ((hand_score.base_mult + bonus.add_mult) as f32 * bonus.mul_mult).max(1.0) as i64;
            let total_gain = hand_score.base_chips * effective_mult + bonus.chip_bonus;

            assert!(
                total_gain > hand_score.base_chips,
                "{} should increase total chips over base {}",
                type_key,
                hand_score.base_chips
            );
        }
    }

    #[test]
    fn unknown_joker_defaults_to_small_bonus() {
        let jokers = vec![joker_slot(1, "???", true)];
        let bonus = compute_joker_bonus(&jokers);
        assert_eq!(bonus.chip_bonus, 2);
        assert_eq!(bonus.add_mult, 0);
        assert!((bonus.mul_mult - 1.0).abs() < f32::EPSILON);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:50051".parse()?;
    let env = EnvService::default();

    println!("JokerEnv gRPC server listening on {}", addr);

    tonic::transport::Server::builder()
        .add_service(JokerEnvServer::new(env))
        .serve(addr)
        .await?;

    Ok(())
}
