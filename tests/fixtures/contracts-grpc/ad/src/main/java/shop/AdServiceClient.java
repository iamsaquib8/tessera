package shop;

import io.grpc.ManagedChannel;

public class AdServiceClient {
  private final AdServiceGrpc.AdServiceBlockingStub blockingStub;

  AdServiceClient(ManagedChannel channel) {
    blockingStub = shop.AdServiceGrpc.newBlockingStub(channel);
  }

  public void fetch(AdRequest request) {
    blockingStub.getAds(request);
  }
}
