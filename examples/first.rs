use dust::prelude::*;

fn my_system(base: In<u32>, count: Res<u32>) {
    println!("{}", base.0 + *count);
}

fn main() {
    let mut dust = dust::dust::Dust::default();
    dust.resources.insert::<u32>(42);

    my_system.into_system().run(&dust, 1);
}
