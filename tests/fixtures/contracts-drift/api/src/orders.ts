import express from 'express';
const router = express.Router();

router.patch('/api/orders/:id', updateOrder);

export function updateOrder(req: any, res: any) {}
