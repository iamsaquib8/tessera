package main

import (
	"context"

	pb "example.com/frontend/genproto"
)

func (fe *frontendServer) getCart(ctx context.Context, userID string) ([]*pb.CartItem, error) {
	resp, err := pb.NewCartServiceClient(fe.cartSvcConn).GetCart(ctx, &pb.GetCartRequest{UserId: userID})
	return resp.GetItems(), err
}

func (fe *frontendServer) convert(ctx context.Context, money *pb.Money) (*pb.Money, error) {
	client := pb.NewCurrencyServiceClient(fe.currencySvcConn)
	return client.
		Convert(ctx, &pb.CurrencyConversionRequest{From: money})
}
