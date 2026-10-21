const grpc = require('@grpc/grpc-js');

function getSupportedCurrencies(call, callback) {
  callback(null, { currency_codes: [] });
}

function convert(call, callback) {
  callback(null, call.request.from);
}

function main() {
  const server = new grpc.Server();
  server.addService(shopProto.CurrencyService.service, {getSupportedCurrencies, convert});
}
