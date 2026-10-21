const { slug, title: toTitle } = require("./strings");
const strings = require("./strings");

function label(user) {
  return toTitle(slug(user.name)) + strings.suffix();
}

module.exports = { label };
