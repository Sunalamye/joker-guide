import grpc
import logging
import os
import queue
import threading
import time
from typing import Iterator, Optional

from .proto import joker_guide_pb2, joker_guide_pb2_grpc

logger = logging.getLogger(__name__)


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

    具備指數退避重連機制（最多 3 次重試），以及 gRPC keepalive 配置。
    """

    # 重連參數
    MAX_RETRIES = 3
    BASE_RETRY_DELAY = 0.1  # 100ms
    MAX_RETRY_DELAY = 2.0   # 2 秒

    # gRPC keepalive 參數
    KEEPALIVE_OPTIONS = [
        ("grpc.keepalive_time_ms", 10000),           # 每 10 秒發送 keepalive ping
        ("grpc.keepalive_timeout_ms", 5000),          # 5 秒內無回應視為斷線
        ("grpc.keepalive_permit_without_calls", 1),   # 無活躍 RPC 時也發送 keepalive
        ("grpc.http2.max_pings_without_data", 0),     # 不限制無資料時的 ping 數量
    ]

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

    def _create_channel(self) -> grpc.Channel:
        """建立帶 keepalive 配置的 gRPC channel。"""
        return grpc.insecure_channel(self._address, options=self.KEEPALIVE_OPTIONS)

    def _teardown_stream(self) -> None:
        """清理現有的 stream 資源（不關閉 channel）。"""
        if self._request_queue and self._stream_active:
            try:
                self._request_queue.put(None)
            except Exception:
                pass
        self._stream_active = False
        self._request_queue = None
        self._response_iterator = None

    def _reconnect(self) -> None:
        """重建 channel 和 stream。訓練中斷後恢復用。"""
        self._teardown_stream()
        # 關閉舊 channel
        if self._channel:
            try:
                self._channel.close()
            except Exception:
                pass
        self._channel = self._create_channel()
        self._stub = joker_guide_pb2_grpc.JokerEnvStub(self._channel)
        self._start_stream_internal()

    def _start_stream_internal(self) -> None:
        """內部方法：啟動 bidirectional stream（不含重試邏輯）。"""
        self._request_queue = queue.Queue()

        def request_iterator() -> Iterator[joker_guide_pb2.StreamRequest]:
            while True:
                try:
                    req = self._request_queue.get(timeout=30.0)
                    if req is None:  # 終止信號
                        break
                    yield req
                except queue.Empty:
                    continue

        self._response_iterator = self._stub.TrainingStream(request_iterator())
        self._stream_active = True

    def _send_and_receive(self, req: joker_guide_pb2.StreamRequest):
        """發送請求並接收回應，帶指數退避重連。"""
        last_error = None
        for attempt in range(self.MAX_RETRIES + 1):
            try:
                with self._lock:
                    self._request_queue.put(req)
                    return next(self._response_iterator)
            except (grpc.RpcError, StopIteration) as e:
                last_error = e
                if attempt < self.MAX_RETRIES:
                    delay = min(
                        self.BASE_RETRY_DELAY * (2 ** attempt),
                        self.MAX_RETRY_DELAY,
                    )
                    logger.warning(
                        "Stream 中斷 (attempt %d/%d), %.1fs 後重連: %s",
                        attempt + 1, self.MAX_RETRIES, delay, e,
                    )
                    time.sleep(delay)
                    try:
                        self._reconnect()
                    except Exception as reconnect_err:
                        logger.warning("重連失敗: %s", reconnect_err)
                        continue

        raise RuntimeError(
            f"Stream 重連失敗，已重試 {self.MAX_RETRIES} 次: {last_error}"
        ) from last_error

    def get_spec(self):
        """獲取環境規格（使用 unary RPC，只在初始化時調用一次）。"""
        if self._channel is None:
            self._channel = self._create_channel()
        if self._stub is None:
            self._stub = joker_guide_pb2_grpc.JokerEnvStub(self._channel)
        request = joker_guide_pb2.GetSpecRequest()
        return self._stub.GetSpec(request)

    def start_stream(self) -> None:
        """啟動 bidirectional stream。"""
        if self._stream_active:
            return

        if self._channel is None:
            self._channel = self._create_channel()
        if self._stub is None:
            self._stub = joker_guide_pb2_grpc.JokerEnvStub(self._channel)
        self._start_stream_internal()

    def reset(self, seed: int = 0) -> joker_guide_pb2.ResetResponse:
        """通過 stream 發送 Reset 請求，帶自動重連。"""
        if not self._stream_active:
            self.start_stream()

        start_ns = time.perf_counter_ns()

        req = joker_guide_pb2.StreamRequest(
            reset=joker_guide_pb2.ResetRequest(seed=seed, session_id=self._session_id)
        )

        response = self._send_and_receive(req)

        if response.HasField('reset'):
            self._session_id = response.reset.session_id
            self._maybe_profile("Reset", start_ns)
            return response.reset
        else:
            raise RuntimeError(f"Expected reset response, got: {response}")

    def step(self, action_id: int, params=None, action_type: int = 0) -> joker_guide_pb2.StepResponse:
        """通過 stream 發送 Step 請求，帶自動重連。"""
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

        response = self._send_and_receive(req)

        if response.HasField('step'):
            self._maybe_profile("Step", start_ns)
            return response.step
        else:
            raise RuntimeError(f"Expected step response, got: {response}")

    def close(self) -> None:
        """關閉 stream 和 channel。"""
        self._teardown_stream()
        if self._channel:
            self._channel.close()
            self._channel = None
        self._stub = None

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
