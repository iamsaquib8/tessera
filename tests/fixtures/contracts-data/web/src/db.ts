export function customers(db: any) {
  return db.query(`SELECT * FROM customers WHERE active`);
}
