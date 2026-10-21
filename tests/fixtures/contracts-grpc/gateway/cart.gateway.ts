import { ChannelCredentials } from '@grpc/grpc-js';
import { CartServiceClient } from './protos/demo';

const client = new CartServiceClient(process.env.CART_ADDR ?? '', ChannelCredentials.createInsecure());

export function addItem(userId: string) {
  return new Promise((resolve) => client.addItem({ userId }, (_e: unknown, r: unknown) => resolve(r)));
}
