// Separate module to check if derive macro correctly
// specifies full paths for the parent crate types.
#[test]
fn test_derive_positive() {
    struct Res1;
    struct Res2;

    struct Sys;

    impl voxbrix_ecs::System for Sys {
        type Args<'a> = Args<'a>;
    }

    #[derive(voxbrix_ecs::SystemArgs)]
    struct Args<'a> {
        res_1: &'a Res1,
        res_2: &'a mut Res2,
    }

    let mut world = voxbrix_ecs::World::new();

    world.add(Res1);
    world.add(Res2);

    let Args { res_1, res_2 } = world.get_args::<Sys>();

    let _ = res_1;
    let _ = res_2;
}
