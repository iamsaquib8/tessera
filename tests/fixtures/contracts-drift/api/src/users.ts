import express from 'express';
const router = express.Router();

router.get('/api/users/:id', getUser);
router.post('/api/users/:id/avatar', uploadAvatar);

export function getUser(req: any, res: any) {}
export function uploadAvatar(req: any, res: any) {}
