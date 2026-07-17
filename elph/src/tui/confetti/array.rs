use rand::RngExt;

pub fn sample<T: Clone>(items: &[T]) -> T {
    let mut rng = rand::rng();
    let index = rng.random_range(0..items.len());
    items[index].clone()
}
