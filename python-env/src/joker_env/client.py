import grpc
import os
import time

from joker_env.proto import joker_guide_pb2, joker_guide_pb2_grpc


class JokerEnvClient:
    def __init__(self, address: str = "127.0.0.1:50051") -> None:
        self._channel = grpc.insecure_channel(address)
        self._stub = joker_guide_pb2_grpc.JokerEnvStub(self._channel)
        self._session_id: int = 0  # 0 表示尚未初始化
        self._grpc_profile_every = int(os.environ.get("JOKER_GRPC_PROFILE_EVERY", "0") or 0)
        self._grpc_counter = 0
        self._last_rpc_ms = 0.0

    def _maybe_profile(self, method: str, start_ns: int) -> None:
        if self._grpc_profile_every <= 0:
            return
        self._grpc_counter += 1
        if self._grpc_counter % self._grpc_profile_every != 0:
            return
        elapsed_ms = (time.perf_counter_ns() - start_ns) / 1_000_000.0
        self._last_rpc_ms = elapsed_ms
        print(
            f"GRPC_PROFILE method={method} session={self._session_id} ms={elapsed_ms:.3f}",
            flush=True,
        )

    def reset(self, seed: int = 0):
        start_ns = time.perf_counter_ns()
        request = joker_guide_pb2.ResetRequest(seed=seed, session_id=self._session_id)
        response = self._stub.Reset(request)
        # 保存 session_id 用於後續請求
        self._session_id = response.session_id
        self._maybe_profile("Reset", start_ns)
        return response

    def step(self, action_id: int, params=None, action_type: int = 0):
        if params is None:
            params = []
        action = joker_guide_pb2.Action(
            action_id=action_id, params=params, action_type=action_type
        )
        request = joker_guide_pb2.StepRequest(action=action, session_id=self._session_id)
        start_ns = time.perf_counter_ns()
        response = self._stub.Step(request)
        self._maybe_profile("Step", start_ns)
        return response

    def step_discard_mask(self, discard_mask: int):
        return self.step(discard_mask, action_type=1)

    def step_play(self):
        return self.step(0, action_type=0)

    def get_spec(self):
        start_ns = time.perf_counter_ns()
        request = joker_guide_pb2.GetSpecRequest()
        response = self._stub.GetSpec(request)
        self._maybe_profile("GetSpec", start_ns)
        return response

    @property
    def session_id(self) -> int:
        return self._session_id

    @property
    def last_rpc_ms(self) -> float:
        return self._last_rpc_ms
