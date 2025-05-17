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

    world.insert(Res1);
    world.insert(Res2);

    let Args { res_1, res_2 } = world.get_args::<Sys>();

    let _ = res_1;
    let _ = res_2;
}

#[test]
fn test_system_tuples() {
    use voxbrix_ecs::{
        System,
        SystemArgs,
        World,
    };

    struct Res1;
    struct Res2;
    struct Res3;
    struct Res4;
    struct Res5;
    struct Res6;

    struct Sys1;
    struct Sys2;
    struct Sys3;

    #[derive(SystemArgs)]
    struct Args1<'a> {
        res_1: &'a Res1,
        res_2: &'a mut Res2,
    }

    #[derive(SystemArgs)]
    struct Args2<'a> {
        res_1: &'a Res3,
        res_2: &'a mut Res4,
    }

    #[derive(SystemArgs)]
    struct Args3<'a> {
        res_1: &'a Res5,
        res_2: &'a mut Res6,
    }

    impl System for Sys1 {
        type Args<'a> = Args1<'a>;
    }

    impl System for Sys2 {
        type Args<'a> = Args2<'a>;
    }

    impl System for Sys3 {
        type Args<'a> = Args3<'a>;
    }

    let mut world = World::new();

    world.insert(Res1);
    world.insert(Res2);
    world.insert(Res3);
    world.insert(Res4);
    world.insert(Res5);
    world.insert(Res6);

    let (args1, args2, args3) = world.get_args::<(Sys1, Sys2, Sys3)>();

    let Args1 { res_1, res_2 } = args1;
    let _: &Res1 = res_1;
    let _: &mut Res2 = res_2;
    let Args2 { res_1, res_2 } = args2;
    let _: &Res3 = res_1;
    let _: &mut Res4 = res_2;
    let Args3 { res_1, res_2 } = args3;
    let _: &Res5 = res_1;
    let _: &mut Res6 = res_2;
}
