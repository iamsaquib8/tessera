import { Kafka } from 'kafkajs';

const kafka = new Kafka({ brokers: ['k:9092'] });
const consumer = kafka.consumer({ groupId: 'notify' });
const producer = kafka.producer();
const SETTLED = 'payments.settled';

export async function start() {
  await consumer.subscribe({ topic: 'orders.created' });
  await producer.send({ topic: SETTLED, messages: [] });
}
