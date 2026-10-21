class BaseWorker {
  run() {
    return this.perform();
  }

  perform() {
    return null;
  }
}

module.exports = BaseWorker;
