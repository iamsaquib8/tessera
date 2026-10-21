import { Controller } from '@nestjs/common';
import { EventPattern } from '@nestjs/microservices';
import { Entity } from 'typeorm';

@Controller()
export class Handlers {
  @EventPattern('payments.failed')
  onFailed(data: unknown) {}
}

@Entity({ name: 'notifications' })
export class Notification {}
