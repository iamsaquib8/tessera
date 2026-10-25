const store = require('./store');

const resolvers = {
  Query: {
    user: (_, { id }) => store.find(id),
    users() {
      return store.all();
    },
  },
  Mutation: {
    createUser: async (_, { name }) => store.add(name),
  },
  User: {
    name: (user) => user.name.trim(),
  },
};

// Not resolvers: a label table that happens to use the same keys.
const labels = { Query: 'Search', Mutation: { title: 'Change' } };

module.exports = { resolvers, labels };
