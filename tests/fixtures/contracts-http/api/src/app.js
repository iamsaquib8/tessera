const express = require('express');
const app = express();
const router = express.Router();
const cache = new Map();

app.get('/api/users/:id', getUser);
router.post('/api/users', (req, res) => res.send(create(req.body)));
cache.get('/api/users/1', null);

function getUser(req, res) {
  res.json({ id: req.params.id });
}

function create(body) {
  return body;
}

module.exports = { app, router };
