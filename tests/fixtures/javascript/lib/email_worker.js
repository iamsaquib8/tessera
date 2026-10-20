const BaseWorker = require("./worker");
const { slugify } = require("./util");

class EmailWorker extends BaseWorker {
  perform() {
    return slugify("Hi");
  }
}

module.exports = EmailWorker;
