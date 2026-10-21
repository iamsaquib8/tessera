function slug(s) {
  return s.toLowerCase();
}

function title(s) {
  return s.toUpperCase();
}

exports.suffix = function suffix() {
  return "!";
};

module.exports.slug = slug;
module.exports.title = title;
