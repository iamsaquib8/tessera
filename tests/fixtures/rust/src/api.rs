use crate::store::{self, Store};

pub fn handle(i: usize) -> bool {
    store::load(i)
}

pub fn load(i: usize) -> bool {
    let s = Store::new();
    s.get(i).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles() {
        assert!(!handle(0));
    }
}
