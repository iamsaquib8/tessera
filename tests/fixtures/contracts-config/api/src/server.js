const port = Number(process.env.PORT || 3000);
const db = process.env['DATABASE_URL'];
const api = import.meta.env.VITE_API_BASE;
module.exports = { port, db, api };
