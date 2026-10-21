#include <grpcpp/grpcpp.h>
#include "demo.grpc.pb.h"

using grpc::ServerContext;
using grpc::Status;

class CurrencyServiceImpl final : public shop::CurrencyService::Service {
  Status Convert(ServerContext* context, const shop::CurrencyConversionRequest* request,
                 shop::Money* response) override {
    return Status::OK;
  }

  int helper() { return 1; }
};

void CheckCart(std::shared_ptr<grpc::Channel> channel) {
  auto stub = shop::CartService::NewStub(channel);
  grpc::ClientContext context;
  shop::Cart cart;
  stub->GetCart(&context, shop::GetCartRequest(), &cart);
}
