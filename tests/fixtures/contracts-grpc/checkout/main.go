package main

import (
	"context"

	pb "example.com/checkout/genproto"
	"google.golang.org/grpc"
)

type checkoutService struct {
	emailSvcConn *grpc.ClientConn
}

func main() {
	srv := grpc.NewServer()
	svc := &checkoutService{}
	pb.RegisterCheckoutServiceServer(srv, svc)
}

func (cs *checkoutService) PlaceOrder(ctx context.Context, req *pb.PlaceOrderRequest) (*pb.PlaceOrderResponse, error) {
	_, err := pb.NewEmailServiceClient(cs.emailSvcConn).SendOrderConfirmation(ctx, &pb.SendOrderConfirmationRequest{})
	return &pb.PlaceOrderResponse{}, err
}
