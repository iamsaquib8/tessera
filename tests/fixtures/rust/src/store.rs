pub struct Store {
    items: Vec<String>,
}

impl Store {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn get(&self, i: usize) -> Option<&String> {
        self.items.get(i)
    }
}

pub fn load(i: usize) -> bool {
    Store::new().get(i).is_some()
}
