//! 遊戲狀態管理

use rand::seq::SliceRandom;
use rand::{rngs::StdRng, Rng, SeedableRng};

use crate::game::{
    Ante, BlindType, BossBlind, Card, Consumable, Enhancement, JokerId, JokerSlot, Seal, Shop, Stage,
    Tag, TagId, ConsumableSlots, HandLevels, VoucherEffects, VoucherId,
    DeckType, DeckConfig, Stake, StakeConfig,
    DISCARDS_PER_BLIND, HAND_SIZE, INTEREST_RATE, JOKER_SLOTS, MAX_INTEREST,
    MONEY_PER_REMAINING_HAND, PLAYS_PER_BLIND, SHOP_JOKER_COUNT, STARTING_MONEY,
    SHOP_PACK_COUNT,
    standard_deck,
};

/// 卡包類型
#[derive(Clone, Debug)]
pub struct BoosterPack {
    pub pack_type: BoosterPackType,
    pub cost: i64,
}

/// 卡包類型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoosterPackType {
    Arcana,     // 2 Tarot 選 1
    Celestial,  // 2 Planet 選 1
    Spectral,   // 2 Spectral 選 1
    Standard,   // 3 普通牌選 1
    Buffoon,    // 2 Joker 選 1
}

impl BoosterPackType {
    /// 轉換為 PackType（用於生成卡包內容）
    pub fn to_pack_type(self) -> crate::game::PackType {
        use crate::game::PackType;
        match self {
            BoosterPackType::Arcana => PackType::Arcana,
            BoosterPackType::Celestial => PackType::Celestial,
            BoosterPackType::Spectral => PackType::Spectral,
            BoosterPackType::Standard => PackType::Standard,
            BoosterPackType::Buffoon => PackType::Buffoon,
        }
    }
}

impl BoosterPack {
    pub fn random(rng: &mut StdRng) -> Self {
        let types = [
            BoosterPackType::Arcana,
            BoosterPackType::Celestial,
            BoosterPackType::Spectral,
            BoosterPackType::Standard,
            BoosterPackType::Buffoon,
        ];
        let pack_type = *types.choose(rng).unwrap();
        let cost = match pack_type {
            BoosterPackType::Arcana => 4,
            BoosterPackType::Celestial => 4,
            BoosterPackType::Spectral => 4,
            BoosterPackType::Standard => 4,
            BoosterPackType::Buffoon => 6,
        };
        Self { pack_type, cost }
    }
}

/// 遊戲環境狀態
pub struct EnvState {
    pub rng: StdRng,

    // 牌組
    pub deck: Vec<Card>,
    pub hand: Vec<Card>,
    pub discarded: Vec<Card>,
    pub selected_mask: u32,

    // Joker
    pub jokers: Vec<JokerSlot>,
    pub joker_slot_limit: usize,

    // 商店
    pub shop: Shop,

    // 遊戲進度
    pub stage: Stage,
    pub blind_type: Option<BlindType>,
    pub boss_blind: Option<BossBlind>,
    pub ante: Ante,
    pub round: i32,

    // 當前 Blind 狀態
    pub plays_left: i32,
    pub discards_left: i32,
    pub score: i64,

    // Boss Blind 追蹤
    pub played_hand_types: Vec<usize>,
    pub first_hand_type: Option<usize>,

    // 經濟
    pub money: i64,
    pub reward: i64,

    // Tags（跳過 Blind 獲得的獎勵）
    pub tags: Vec<Tag>,

    // 無盡模式
    pub endless_mode: bool,
    pub endless_ante: i32, // Ante 8 之後的額外等級

    // 消耗品
    pub consumables: ConsumableSlots,
    pub hand_levels: HandLevels,

    // Voucher
    pub voucher_effects: VoucherEffects,
    pub shop_voucher: Option<VoucherId>,

    // 卡包
    pub shop_packs: Vec<BoosterPack>,

    // 牌組和難度
    pub deck_type: DeckType,
    pub stake: Stake,

    // 統計
    pub episode_step: i32,

    // Run 追蹤（用於 Joker 效果計算）
    pub rerolls_this_run: i32,    // Flash Joker: +2 Mult per reroll
    pub blinds_skipped: i32,      // RedCard: +3 Mult per skip
    pub planets_used_this_run: i32, // Satellite: +$1 per unique Planet used
    pub tarots_used_this_run: i32,  // Fortune_Teller: +1 Mult per Tarot used

    // Blind 追蹤
    pub discards_used_this_blind: i32, // Delayed: +$2 if no discards used
    pub hands_played_this_blind: i32,  // DNA: X2 分數 if first hand

    // Obelisk: 牌型計數（用於確定最常打的牌型）
    pub hand_type_counts: [i32; 13],

    // ThePillar: 追蹤已打過的牌 (rank, suit)
    pub pillar_played_cards: std::collections::HashSet<(u8, u8)>,

    // Spectral 永久效果
    pub hand_size_modifier: i32, // Ouija/Ectoplasm: 永久手牌大小修改

    // TheFool: 上一張使用的消耗品
    pub last_used_consumable: Option<Consumable>,

    // Observatory: 追蹤使用過 Planet 的牌型 (bitmask)
    pub planet_used_hand_types: u16,
}

impl EnvState {
    pub fn new(seed: u64) -> Self {
        let rng = StdRng::seed_from_u64(seed);
        let deck = standard_deck();

        Self {
            rng,
            deck,
            hand: Vec::new(),
            discarded: Vec::new(),
            selected_mask: 0,
            jokers: Vec::new(),
            joker_slot_limit: JOKER_SLOTS,
            shop: Shop::new(),
            stage: Stage::PreBlind,
            blind_type: None,
            boss_blind: None,
            ante: Ante::One,
            round: 1,
            plays_left: PLAYS_PER_BLIND,
            discards_left: DISCARDS_PER_BLIND,
            score: 0,
            played_hand_types: Vec::new(),
            first_hand_type: None,
            money: STARTING_MONEY,
            reward: 0,
            tags: Vec::new(),
            endless_mode: false,
            endless_ante: 0,
            consumables: ConsumableSlots::new(),
            hand_levels: HandLevels::new(),
            voucher_effects: VoucherEffects::new(),
            shop_voucher: None,
            shop_packs: Vec::new(),
            deck_type: DeckType::Standard,
            stake: Stake::White,
            episode_step: 0,
            rerolls_this_run: 0,
            blinds_skipped: 0,
            planets_used_this_run: 0,
            tarots_used_this_run: 0,
            discards_used_this_blind: 0,
            hands_played_this_blind: 0,
            hand_type_counts: [0; 13],
            pillar_played_cards: std::collections::HashSet::new(),
            hand_size_modifier: 0,
            last_used_consumable: None,
            planet_used_hand_types: 0,
        }
    }

    /// 創建指定牌組和難度的遊戲
    #[allow(dead_code)]
    pub fn new_with_config(seed: u64, deck_type: DeckType, stake: Stake) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let deck_config = DeckConfig::from_deck_type(deck_type);
        let stake_config = StakeConfig::from_stake(stake);
        let deck = deck_type.create_deck(&mut rng);

        Self {
            rng,
            deck,
            hand: Vec::new(),
            discarded: Vec::new(),
            selected_mask: 0,
            jokers: Vec::new(),
            joker_slot_limit: deck_config.joker_slots,
            shop: Shop::new(),
            stage: Stage::PreBlind,
            blind_type: None,
            boss_blind: None,
            ante: Ante::One,
            round: 1,
            plays_left: deck_config.plays_per_blind + stake_config.hand_modifier,
            discards_left: deck_config.discards_per_blind + stake_config.discard_modifier,
            score: 0,
            played_hand_types: Vec::new(),
            first_hand_type: None,
            money: deck_config.starting_money,
            reward: 0,
            tags: Vec::new(),
            endless_mode: false,
            endless_ante: 0,
            consumables: ConsumableSlots::new(),
            hand_levels: HandLevels::new(),
            voucher_effects: VoucherEffects::new(),
            shop_voucher: None,
            shop_packs: Vec::new(),
            deck_type,
            stake,
            episode_step: 0,
            rerolls_this_run: 0,
            blinds_skipped: 0,
            planets_used_this_run: 0,
            tarots_used_this_run: 0,
            discards_used_this_blind: 0,
            hands_played_this_blind: 0,
            hand_type_counts: [0; 13],
            pillar_played_cards: std::collections::HashSet::new(),
            hand_size_modifier: 0,
            last_used_consumable: None,
            planet_used_hand_types: 0,
        }
    }

    /// 創建無盡模式遊戲
    #[allow(dead_code)]
    pub fn new_endless(seed: u64) -> Self {
        let mut state = Self::new(seed);
        state.endless_mode = true;
        state
    }

    /// 創建指定配置的無盡模式遊戲
    #[allow(dead_code)]
    pub fn new_endless_with_config(seed: u64, deck_type: DeckType, stake: Stake) -> Self {
        let mut state = Self::new_with_config(seed, deck_type, stake);
        state.endless_mode = true;
        state
    }

    /// 跳過當前 Blind 並獲得隨機 Tag
    pub fn skip_blind(&mut self) -> Option<Tag> {
        // 只能跳過 Small 和 Big Blind
        match self.blind_type {
            Some(BlindType::Small) | Some(BlindType::Big) | None => {}
            Some(BlindType::Boss) => return None,
        }

        // 獲得隨機 Tag
        let tag_id = TagId::random(&mut self.rng);
        let tag = Tag::new(tag_id);

        // 立即獲得金幣獎勵
        self.money += tag.id.immediate_money();

        self.tags.push(tag.clone());

        // 進入下一個 Blind
        let next_blind = self.blind_type
            .and_then(|b| b.next())
            .unwrap_or(BlindType::Small);

        if next_blind == BlindType::Boss {
            // 跳到 Boss Blind
            self.blind_type = Some(BlindType::Boss);
            self.select_random_boss();
        } else {
            self.blind_type = Some(next_blind);
        }

        // 進入下一輪（跳過的 Blind 不計入 round）
        self.stage = Stage::PreBlind;

        Some(tag)
    }

    pub fn required_score(&self) -> i64 {
        let base = self.ante.base_score();
        let blind_mult = if self.blind_type == Some(BlindType::Boss) {
            self.boss_blind.map(|b| b.score_multiplier()).unwrap_or(2.0)
        } else {
            self.blind_type.map(|b| b.score_multiplier()).unwrap_or(1.0)
        };

        // Green Stake 及以上: +25% 分數需求
        let stake_mult = self.stake.score_multiplier();

        // 無盡模式的額外倍數（每額外 Ante +50%）
        let endless_mult = if self.endless_mode && self.endless_ante > 0 {
            1.5f32.powi(self.endless_ante)
        } else {
            1.0
        };

        (base as f32 * blind_mult * stake_mult * endless_mult) as i64
    }

    /// 進入下一個 Ante（支援無盡模式和 Voucher ante 減免）
    pub fn advance_ante(&mut self) -> bool {
        // 計算目標 Ante（考慮 Hieroglyph/Petroglyph 減免）
        // 正常: 需要通過 Ante 8
        // -1 減免: 只需通過 Ante 7
        // -2 減免: 只需通過 Ante 6
        let target_ante = (8 - self.voucher_effects.ante_reduction).max(1);

        if self.ante.to_int() >= target_ante && !self.endless_mode {
            // 已達到目標 Ante，遊戲勝利
            return false;
        }

        if let Some(next_ante) = self.ante.next() {
            self.ante = next_ante;
            true
        } else if self.endless_mode {
            // 無盡模式：保持 Ante 8，但增加 endless_ante
            self.endless_ante += 1;
            true
        } else {
            // 遊戲勝利
            false
        }
    }

    pub fn deal(&mut self) {
        self.deck.append(&mut self.hand);
        self.deck.append(&mut self.discarded);
        self.deck.shuffle(&mut self.rng);
        let hand_size = self.effective_hand_size();
        self.hand = self.deck.drain(0..hand_size.min(self.deck.len())).collect();
        self.selected_mask = 0;
    }

    /// 計算完整獎勵金
    /// 包含：基礎獎勵 + 利息 + 剩餘手牌獎勵 + Gold 卡加成 + Joker 加成
    pub fn calc_reward(&self) -> i64 {
        let blind = self.blind_type.unwrap_or(BlindType::Small);

        // 基礎獎勵（根據 Blind 類型）
        // Red Stake 及以上：Small Blind 不給基礎獎勵
        let base = if blind == BlindType::Small && !self.stake.small_blind_gives_reward() {
            0
        } else {
            blind.reward()
        };

        // 利息（10%，最高 $5）
        // Green Deck: 無利息
        let interest = if self.deck_type.disables_interest() {
            0
        } else {
            let base_cap = MAX_INTEREST + self.voucher_effects.interest_cap_bonus;
            ((self.money as f32 * INTEREST_RATE).floor() as i64).min(base_cap)
        };

        // 剩餘出牌獎勵
        // Green Deck: 每剩餘手牌 +$2（預設 $1）
        let money_per_hand = self.deck_type.money_per_remaining_hand();
        let hand_bonus = self.plays_left as i64 * money_per_hand;

        // Gold 卡加成（手牌中每張 Gold 卡 +$3）
        let gold_bonus = self.gold_card_money();

        // Joker 金幣加成（TODO: 實作更多金幣 Joker）
        let joker_bonus = self.calc_joker_money_bonus();

        base + interest + hand_bonus + gold_bonus + joker_bonus
    }

    /// 計算 Joker 金幣加成（回合結束時）
    ///
    /// 包含所有在回合結束時給予金幣的 Joker：
    /// - Banner: 每剩餘棄牌 +$2
    /// - GoldenJoker: +$4
    /// - ToTheMoon: 持有 $5+ 時 +$1
    /// - CloudNine: 牌組中每張 9 +$1
    /// - Rocket: +$1（會隨時間縮放）
    /// - Satellite: 每使用過的 Planet +$1
    /// - Delayed: 此 Blind 沒有棄牌時 +$2
    /// - Golden_Ticket: 手牌中每張 Gold 卡 +$3（已在 gold_card_money 中）
    fn calc_joker_money_bonus(&self) -> i64 {
        use crate::game::JokerId;
        let mut bonus = 0i64;

        for joker in &self.jokers {
            if !joker.enabled {
                continue;
            }
            match joker.id {
                JokerId::Banner => {
                    // Banner: 每剩餘棄牌 +$2
                    bonus += self.discards_left as i64 * 2;
                }
                JokerId::GoldenJoker => {
                    // GoldenJoker: 回合結束 +$4
                    bonus += 4;
                }
                JokerId::ToTheMoon => {
                    // ToTheMoon: 回合結束持有 $5+ 時 +$1
                    if self.money >= 5 {
                        bonus += 1;
                    }
                }
                JokerId::CloudNine | JokerId::Cloud9 => {
                    // CloudNine: 牌組中每張 9 +$1
                    let nine_count = self.deck.iter()
                        .filter(|c| c.rank == 9)
                        .count() as i64;
                    bonus += nine_count;
                }
                JokerId::Rocket => {
                    // Rocket: 回合結束 +$1（基礎值，實際會隨 Boss 過關縮放）
                    // 這裡使用 joker.counter 追蹤累積的額外金幣
                    bonus += 1 + joker.counter as i64;
                }
                JokerId::Satellite => {
                    // Satellite: 每使用過的 Planet +$1
                    bonus += self.planets_used_this_run as i64;
                }
                JokerId::Delayed => {
                    // Delayed: 此 Blind 沒有棄牌時 +$2
                    if self.discards_used_this_blind == 0 {
                        bonus += 2;
                    }
                }
                _ => {}
            }
        }

        bonus
    }

    pub fn refresh_shop(&mut self) {
        self.shop.refresh(&mut self.rng, SHOP_JOKER_COUNT);

        // Black Stake 及以上：商店 Joker 有 30% 機率為 Eternal
        if self.stake.has_eternal_jokers() {
            for item in &mut self.shop.items {
                if self.rng.gen_range(0..10) < 3 {
                    item.joker.is_eternal = true;
                }
            }
        }

        // Orange Stake 及以上：非 Eternal 的 Joker 有 30% 機率為 Perishable
        if self.stake.has_perishable_jokers() {
            for item in &mut self.shop.items {
                if !item.joker.is_eternal && self.rng.gen_range(0..10) < 3 {
                    item.joker.is_perishable = true;
                    item.joker.perishable_rounds = 5;
                }
            }
        }

        // 生成 Voucher（如果還有可購買的）
        self.shop_voucher = VoucherId::random_available(&mut self.rng, &self.voucher_effects.owned);

        // 生成卡包
        self.shop_packs.clear();
        for _ in 0..SHOP_PACK_COUNT {
            self.shop_packs.push(BoosterPack::random(&mut self.rng));
        }
    }

    /// Reroll 商店（增加 reroll 計數並刷新 Joker）
    /// Voucher 和卡包不會被 reroll 影響
    pub fn reroll_shop(&mut self) {
        self.shop.reroll_count += 1;
        self.shop.refresh(&mut self.rng, SHOP_JOKER_COUNT);

        // Black Stake 及以上：商店 Joker 有 30% 機率為 Eternal
        if self.stake.has_eternal_jokers() {
            for item in &mut self.shop.items {
                if self.rng.gen_range(0..10) < 3 {
                    item.joker.is_eternal = true;
                }
            }
        }

        // Orange Stake 及以上：非 Eternal 的 Joker 有 30% 機率為 Perishable
        if self.stake.has_perishable_jokers() {
            for item in &mut self.shop.items {
                if !item.joker.is_eternal && self.rng.gen_range(0..10) < 3 {
                    item.joker.is_perishable = true;
                    item.joker.perishable_rounds = 5;
                }
            }
        }
    }

    /// TheHook: 隨機棄 2 張手牌
    pub fn apply_hook_discard(&mut self) {
        let discard_count = 2.min(self.hand.len());
        for _ in 0..discard_count {
            if self.hand.is_empty() { break; }
            let idx = self.rng.gen_range(0..self.hand.len());
            let card = self.hand.remove(idx);
            self.discarded.push(card);
        }
        let hand_size = self.effective_hand_size();
        let draw_count = hand_size.saturating_sub(self.hand.len());
        for _ in 0..draw_count {
            if let Some(card) = self.deck.pop() {
                self.hand.push(card);
            }
        }
    }

    /// TheSerpent: 抽 3 張，棄 3 張
    pub fn apply_serpent_effect(&mut self) {
        for _ in 0..3 {
            if let Some(card) = self.deck.pop() {
                self.hand.push(card);
            }
        }
        let discard_count = 3.min(self.hand.len());
        for _ in 0..discard_count {
            if self.hand.is_empty() { break; }
            let idx = self.rng.gen_range(0..self.hand.len());
            let card = self.hand.remove(idx);
            self.discarded.push(card);
        }
    }

    /// 選擇隨機 Boss Blind
    pub fn select_random_boss(&mut self) {
        let bosses = if self.ante == Ante::Eight {
            BossBlind::showdown_bosses()
        } else {
            BossBlind::regular_bosses()
        };
        self.boss_blind = bosses.choose(&mut self.rng).copied();
    }

    /// 處理 Glass 牌破碎
    pub fn break_glass_cards(&mut self, selected_mask: u32, glass_indices: &[usize]) {
        if glass_indices.is_empty() {
            return;
        }

        let mut selected_idx = 0;
        let mut to_remove = Vec::new();

        for (hand_idx, _) in self.hand.iter().enumerate() {
            if ((selected_mask >> hand_idx) & 1) == 1 {
                if glass_indices.contains(&selected_idx) {
                    to_remove.push(hand_idx);
                }
                selected_idx += 1;
            }
        }

        to_remove.sort_unstable_by(|a, b| b.cmp(a));
        for idx in to_remove {
            self.hand.remove(idx);
        }
    }

    /// 處理棄牌（含 Purple Seal 效果）
    pub fn discard_with_seals(&mut self, mask: u32) -> i32 {
        let mut purple_count = 0;

        let mut new_hand = Vec::new();
        for (idx, card) in self.hand.iter().enumerate() {
            if ((mask >> idx) & 1) == 1 {
                if card.seal == Seal::Purple {
                    purple_count += 1;
                }
                self.discarded.push(*card);
            } else {
                new_hand.push(*card);
            }
        }

        let hand_size = self.effective_hand_size();
        let draw_count = hand_size.saturating_sub(new_hand.len());
        for _ in 0..draw_count {
            if let Some(card) = self.deck.pop() {
                new_hand.push(card);
            }
        }

        self.hand = new_hand;
        purple_count
    }

    /// 計算 Steel 牌的 mult 加成
    #[allow(dead_code)]
    pub fn steel_mult_bonus(&self) -> f32 {
        let mut x_mult = 1.0;
        for card in &self.hand {
            if card.enhancement == Enhancement::Steel {
                x_mult *= 1.5;
            }
        }
        x_mult
    }

    /// 計算 Gold 牌的回合結束金幣
    pub fn gold_card_money(&self) -> i64 {
        self.hand
            .iter()
            .filter(|c| c.enhancement == Enhancement::Gold)
            .count() as i64 * 3
    }

    /// 計算有效手牌大小（考慮 Joker 修正）
    /// - Juggler: +1
    /// 計算有效 Joker 槽位上限
    ///
    /// 考慮：
    /// - Voucher 效果 (Blank, Antimatter)
    /// - Negative edition 的 Joker 不佔槽位，等同於 +1 槽位
    pub fn effective_joker_slot_limit(&self) -> usize {
        let base = self.joker_slot_limit;
        let voucher_bonus = self.voucher_effects.joker_slot_bonus as usize;
        let negative_count = self.jokers.iter()
            .filter(|j| j.is_negative)
            .count();
        base + voucher_bonus + negative_count
    }

    /// 計算有效手牌大小
    ///
    /// 考慮 Joker 效果:
    /// - Juggler: +1
    /// - Troubadour: +2
    /// - Stuntman: -2
    /// - TurtleBean: 由 turtle_hand_mod 追蹤
    /// - TheManacle (Boss): -1
    /// - Purple Stake 及以上: -1
    pub fn effective_hand_size(&self) -> usize {
        let base = HAND_SIZE as i32;
        let mut modifier: i32 = 0;

        for joker in &self.jokers {
            if !joker.enabled {
                continue;
            }
            match joker.id {
                JokerId::Juggler => modifier += 1,
                JokerId::Troubadour => modifier += 2,
                JokerId::Stuntman => modifier -= 2,
                JokerId::TurtleBean => modifier += joker.turtle_hand_mod,
                _ => {}
            }
        }

        // Boss Blind 效果
        if self.boss_blind == Some(BossBlind::TheManacle) {
            modifier -= 1;
        }

        // Spectral 永久效果 (Ouija, Ectoplasm)
        modifier += self.hand_size_modifier;

        // Purple Stake 及以上: -1 手牌大小
        modifier += self.stake.hand_size_modifier();

        (base + modifier).max(1) as usize
    }

    // =========================================================================
    // 卡牌操作系統 (Card Manipulation System)
    // =========================================================================

    /// 銷毀手牌中指定索引的卡牌
    ///
    /// 觸發 Joker 效果:
    /// - Canio/Caino: 銷毀人頭牌時 +X Mult
    ///
    /// 返回銷毀的人頭牌數量（用於 Joker 狀態更新）
    pub fn destroy_cards_from_hand(&mut self, indices: &[usize]) -> i32 {
        if indices.is_empty() {
            return 0;
        }

        let mut face_cards_destroyed = 0;

        // 計算銷毀的人頭牌數量
        for &idx in indices {
            if idx < self.hand.len() {
                let card = &self.hand[idx];
                if card.is_face() {
                    face_cards_destroyed += 1;
                }
            }
        }

        // 從大到小排序，避免索引偏移問題
        let mut sorted_indices: Vec<usize> = indices.to_vec();
        sorted_indices.sort_unstable_by(|a, b| b.cmp(a));
        sorted_indices.dedup();

        // 移除卡牌
        for idx in sorted_indices {
            if idx < self.hand.len() {
                self.hand.remove(idx);
            }
        }

        // 更新 Joker 狀態（Canio/Caino）
        if face_cards_destroyed > 0 {
            for joker in &mut self.jokers {
                if joker.enabled {
                    joker.update_canio_on_face_destroyed(face_cards_destroyed);
                }
            }
        }

        face_cards_destroyed
    }

    /// 將新卡牌加入牌組
    ///
    /// 觸發 Joker 效果:
    /// - Hologram: 每加入一張牌 +0.25 X Mult
    ///
    /// 返回實際加入的卡牌數量
    pub fn add_cards_to_deck(&mut self, cards: Vec<Card>) -> usize {
        let count = cards.len();

        // 加入牌組
        self.deck.extend(cards);

        // 更新 Joker 狀態（Hologram）
        if count > 0 {
            for joker in &mut self.jokers {
                if joker.enabled {
                    joker.update_hologram_on_card_added(count as i32);
                }
            }
        }

        count
    }

    /// 增強手牌中指定索引的卡牌
    ///
    /// 用於 Tarot 卡效果:
    /// - The Magician (Lucky), The Empress (Mult), The Hierophant (Bonus)
    /// - The Lovers (Wild), The Chariot (Steel), Justice (Glass)
    /// - The Tower (Stone), The Devil (Gold)
    pub fn enhance_cards(&mut self, indices: &[usize], enhancement: Enhancement) {
        for &idx in indices {
            if idx < self.hand.len() {
                self.hand[idx].enhancement = enhancement;
            }
        }
    }

    /// 為手牌中指定索引的卡牌添加封印
    ///
    /// 用於 Spectral 卡效果:
    /// - Deja Vu (Red), Trance (Blue), Medium (Purple), Talisman (Gold)
    pub fn add_seals_to_cards(&mut self, indices: &[usize], seal: Seal) {
        for &idx in indices {
            if idx < self.hand.len() {
                self.hand[idx].seal = seal;
            }
        }
    }

    /// 轉換手牌中指定索引卡牌的花色
    ///
    /// 用於 Tarot 卡效果:
    /// - The World (Spades), The Star (Diamonds)
    /// - The Moon (Clubs), The Sun (Hearts)
    pub fn change_card_suits(&mut self, indices: &[usize], suit: u8) {
        for &idx in indices {
            if idx < self.hand.len() {
                self.hand[idx].suit = suit;
            }
        }
    }

    /// 轉換所有手牌為同一花色
    ///
    /// 用於 Spectral - Sigil
    pub fn convert_all_hand_to_suit(&mut self, suit: u8) {
        for card in &mut self.hand {
            card.suit = suit;
        }
    }

    /// 轉換所有手牌為同一點數
    ///
    /// 用於 Spectral - Ouija（會 -1 手牌大小）
    pub fn convert_all_hand_to_rank(&mut self, rank: u8) {
        for card in &mut self.hand {
            card.rank = rank;
        }
        // 注意：Ouija 的 -1 手牌大小效果應在調用處處理
        // self.hand_size_modifier -= 1;
    }

    /// 複製手牌中指定索引的卡牌（加入牌組）
    ///
    /// 用於 Spectral - Cryptid
    /// 觸發 Hologram 效果
    pub fn copy_cards_to_deck(&mut self, indices: &[usize]) -> usize {
        let cards_to_copy: Vec<Card> = indices.iter()
            .filter_map(|&idx| self.hand.get(idx).copied())
            .collect();

        self.add_cards_to_deck(cards_to_copy)
    }

    /// 銷毀手牌並添加隨機人頭牌到牌組
    ///
    /// 用於 Spectral - Familiar
    /// 返回 (銷毀的人頭牌數, 添加的卡牌數)
    pub fn familiar_effect(&mut self, destroy_idx: usize, add_count: usize) -> (i32, usize) {
        let destroyed = self.destroy_cards_from_hand(&[destroy_idx]);

        // 生成隨機人頭牌 (J=11, Q=12, K=13)
        let face_ranks = [11u8, 12, 13];
        let suits = [0u8, 1, 2, 3]; // Spades, Hearts, Diamonds, Clubs

        let mut new_cards = Vec::with_capacity(add_count);
        for _ in 0..add_count {
            let rank = *face_ranks.choose(&mut self.rng).unwrap();
            let suit = *suits.choose(&mut self.rng).unwrap();
            new_cards.push(Card::new(rank, suit));
        }

        let added = self.add_cards_to_deck(new_cards);
        (destroyed, added)
    }

    /// 銷毀手牌並添加隨機 Ace 到牌組
    ///
    /// 用於 Spectral - Grim
    /// 返回 (銷毀的人頭牌數, 添加的卡牌數)
    pub fn grim_effect(&mut self, destroy_idx: usize, add_count: usize) -> (i32, usize) {
        let destroyed = self.destroy_cards_from_hand(&[destroy_idx]);

        // 生成隨機 Ace
        let suits = [0u8, 1, 2, 3];

        let mut new_cards = Vec::with_capacity(add_count);
        for _ in 0..add_count {
            let suit = *suits.choose(&mut self.rng).unwrap();
            new_cards.push(Card::new(1, suit)); // Ace = rank 1
        }

        let added = self.add_cards_to_deck(new_cards);
        (destroyed, added)
    }

    /// 銷毀手牌並添加隨機數字牌到牌組
    ///
    /// 用於 Spectral - Incantation
    /// 返回 (銷毀的人頭牌數, 添加的卡牌數)
    pub fn incantation_effect(&mut self, destroy_idx: usize, add_count: usize) -> (i32, usize) {
        let destroyed = self.destroy_cards_from_hand(&[destroy_idx]);

        // 生成隨機數字牌 (2-10)
        let number_ranks: Vec<u8> = (2..=10).collect();
        let suits = [0u8, 1, 2, 3];

        let mut new_cards = Vec::with_capacity(add_count);
        for _ in 0..add_count {
            let rank = *number_ranks.choose(&mut self.rng).unwrap();
            let suit = *suits.choose(&mut self.rng).unwrap();
            new_cards.push(Card::new(rank, suit));
        }

        let added = self.add_cards_to_deck(new_cards);
        (destroyed, added)
    }

    /// Immolate 效果：銷毀 5 張手牌，獲得 $20
    ///
    /// 返回銷毀的人頭牌數量
    pub fn immolate_effect(&mut self, indices: &[usize]) -> i32 {
        let destroyed = self.destroy_cards_from_hand(indices);
        self.money += 20;
        destroyed
    }
}
