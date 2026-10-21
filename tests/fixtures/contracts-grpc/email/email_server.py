import demo_pb2
import demo_pb2_grpc


class EmailService(demo_pb2_grpc.EmailServiceServicer):
    def SendOrderConfirmation(self, request, context):
        return demo_pb2.Empty()

    def _render(self, order):
        return ""
