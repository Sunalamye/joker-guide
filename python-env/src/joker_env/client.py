import grpc

from joker_env.proto import joker_guide_pb2, joker_guide_pb2_grpc


class JokerEnvClient:
    def __init__(self, address: str = "127.0.0.1:50051") -> None:
        self._channel = grpc.insecure_channel(address)
        self._stub = joker_guide_pb2_grpc.JokerEnvStub(self._channel)
        self._session_id: int = 0  # 0 表示尚未初始化

    def reset(self, seed: int = 0):
        request = joker_guide_pb2.ResetRequest(seed=seed, session_id=self._session_id)
        response = self._stub.Reset(request)
        # 保存 session_id 用於後續請求
        self._session_id = response.session_id
        return response

    def step(self, action_id: int, params=None, action_type: int = 0):
        if params is None:
            params = []
        action = joker_guide_pb2.Action(
            action_id=action_id, params=params, action_type=action_type
        )
        request = joker_guide_pb2.StepRequest(action=action, session_id=self._session_id)
        return self._stub.Step(request)

    def step_discard_mask(self, discard_mask: int):
        return self.step(discard_mask, action_type=1)

    def step_play(self):
        return self.step(0, action_type=0)

    def get_spec(self):
        request = joker_guide_pb2.GetSpecRequest()
        return self._stub.GetSpec(request)

    @property
    def session_id(self) -> int:
        return self._session_id
