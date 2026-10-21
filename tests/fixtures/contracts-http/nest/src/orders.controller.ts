import { Controller, Get, Post, Param } from '@nestjs/common';

@Controller('api/orders')
export class OrdersController {
  @Get(':id')
  findOne(@Param('id') id: string) {
    return { id };
  }

  @Post()
  create() {
    return {};
  }
}
