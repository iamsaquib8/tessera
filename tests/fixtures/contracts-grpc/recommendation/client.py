import grpc
import demo_pb2
import demo_pb2_grpc


def list_products(channel):
    stub = demo_pb2_grpc.ProductCatalogServiceStub(channel)
    return stub.ListProducts(demo_pb2.Empty())
