import grpc
import os
import queue
import threading
import time
from typing import Iterator, Optional

from .proto import joker_guide_pb2, joker_guide_pb2_grpc


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

    def reset_with_session(self, session_id: int, seed: int = 0):
        start_ns = time.perf_counter_ns()
        request = joker_guide_pb2.ResetRequest(seed=seed, session_id=session_id)
        response = self._stub.Reset(request)
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

    def step_with_session(self, session_id: int, action_id: int, params=None, action_type: int = 0):
        if params is None:
            params = []
        action = joker_guide_pb2.Action(
            action_id=action_id, params=params, action_type=action_type
        )
        request = joker_guide_pb2.StepRequest(action=action, session_id=session_id)
        start_ns = time.perf_counter_ns()
        response = self._stub.Step(request)
        self._maybe_profile("Step", start_ns)
        return response

    def step_batch(self, requests):
        start_ns = time.perf_counter_ns()
        batch = joker_guide_pb2.StepBatchRequest(requests=requests)
        response = self._stub.StepBatch(batch)
        self._maybe_profile("StepBatch", start_ns)
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


class StreamingJokerClient:
    """Bidirectional streaming client for low-latency training.

    使用持久的 gRPC stream 連接，避免每次請求的連接開銷。
    預期延遲：0.1-0.4 ms/step（相比 unary RPC 的 3-10 ms）。
    """

    def __init__(self, address: str = "127.0.0.1:50051") -> None:
        self._address = address
        self._channel: Optional[grpc.Channel] = None
        self._stub: Optional[joker_guide_pb2_grpc.JokerEnvStub] = None
        self._request_queue: Optional[queue.Queue] = None
        self._response_iterator: Optional[Iterator] = None
        self._session_id: int = 0
        self._stream_active: bool = False
        self._lock = threading.Lock()

        # Profiling
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
            f"GRPC_STREAM_PROFILE method={method} session={self._session_id} ms={elapsed_ms:.3f}",
            flush=True,
        )

    def start_stream(self) -> None:
        """啟動 bidirectional stream。"""
        if self._stream_active:
            return

        self._channel = grpc.insecure_channel(self._address)
        self._stub = joker_guide_pb2_grpc.JokerEnvStub(self._channel)
        self._request_queue = queue.Queue()

        def request_iterator() -> Iterator[joker_guide_pb2.StreamRequest]:
            while True:
                try:
                    req = self._request_queue.get(timeout=30.0)
                    if req is None:  # 終止信號
                        break
                    yield req
                except queue.Empty:
                    # 超時，繼續等待
                    continue

        self._response_iterator = self._stub.TrainingStream(request_iterator())
        self._stream_active = True

    def reset(self, seed: int = 0) -> joker_guide_pb2.ResetResponse:
        """通過 stream 發送 Reset 請求。"""
        if not self._stream_active:
            self.start_stream()

        start_ns = time.perf_counter_ns()

        req = joker_guide_pb2.StreamRequest(
            reset=joker_guide_pb2.ResetRequest(seed=seed, session_id=self._session_id)
        )

        with self._lock:
            self._request_queue.put(req)
            response = next(self._response_iterator)

        if response.HasField('reset'):
            self._session_id = response.reset.session_id
            self._maybe_profile("Reset", start_ns)
            return response.reset
        else:
            raise RuntimeError(f"Expected reset response, got: {response}")

    def step(self, action_id: int, params=None, action_type: int = 0) -> joker_guide_pb2.StepResponse:
        """通過 stream 發送 Step 請求。"""
        if not self._stream_active:
            raise RuntimeError("Stream not started. Call start_stream() first, or use reset().")

        if params is None:
            params = []

        start_ns = time.perf_counter_ns()

        action = joker_guide_pb2.Action(
            action_id=action_id,
            params=params,
            action_type=action_type,
        )
        req = joker_guide_pb2.StreamRequest(
            step=joker_guide_pb2.StepRequest(action=action, session_id=self._session_id)
        )

        with self._lock:
            self._request_queue.put(req)
            response = next(self._response_iterator)

        if response.HasField('step'):
            self._maybe_profile("Step", start_ns)
            return response.step
        else:
            raise RuntimeError(f"Expected step response, got: {response}")

    def close(self) -> None:
        """關閉 stream。"""
        if self._request_queue and self._stream_active:
            self._request_queue.put(None)  # 發送終止信號
            self._stream_active = False

        if self._channel:
            self._channel.close()
            self._channel = None

    @property
    def session_id(self) -> int:
        return self._session_id

    @property
    def last_rpc_ms(self) -> float:
        return self._last_rpc_ms

    @property
    def is_active(self) -> bool:
        return self._stream_active

    def __enter__(self):
        self.start_stream()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()
        return False
