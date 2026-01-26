import grpc

from joker_env.proto import joker_guide_pb2, joker_guide_pb2_grpc


class JokerEnvClient:
    def __init__(self, address: str = "127.0.0.1:50051") -> None:
        self._channel = grpc.insecure_channel(address)
        self._stub = joker_guide_pb2_grpc.JokerEnvStub(self._channel)

    def reset(self, seed: int = 0):
        request = joker_guide_pb2.ResetRequest(seed=seed)
        return self._stub.Reset(request)

    def step(self, action_id: int, params=None, action_type: int = 0):
        if params is None:
            params = []
        action = joker_guide_pb2.Action(
            action_id=action_id, params=params, action_type=action_type
        )
        request = joker_guide_pb2.StepRequest(action=action)
        return self._stub.Step(request)

    def step_discard_mask(self, discard_mask: int):
        return self.step(discard_mask, action_type=1)

    def step_play(self):
        return self.step(0, action_type=0)

    def get_spec(self):
        request = joker_guide_pb2.GetSpecRequest()
        return self._stub.GetSpec(request)
