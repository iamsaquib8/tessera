const express = require("express");
const EmailWorker = require("./lib/email_worker");

const app = express();

app.get("/send", (req, res) => {
  new EmailWorker().run();
  res.send("ok");
});

function slugify(x) {
  return x;
}

module.exports = app;
