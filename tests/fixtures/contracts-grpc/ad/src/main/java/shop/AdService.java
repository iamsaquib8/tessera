package shop;

import io.grpc.stub.StreamObserver;

public final class AdService {
  private static class AdServiceImpl extends shop.AdServiceGrpc.AdServiceImplBase {
    @Override
    public void getAds(AdRequest req, StreamObserver<AdResponse> responseObserver) {
      responseObserver.onCompleted();
    }
  }
}
