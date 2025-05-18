use std::any::{
    Any,
    TypeId,
};
use voxbrix_ecs::{
    Access,
    Request,
    System,
    SystemArgs,
    World,
};

#[test]
fn test_positive() {
    struct Res1;
    struct Res2;

    struct Sys;

    impl System for Sys {
        type Args<'a> = Args<'a>;
    }

    struct Args<'a> {
        res_1: &'a Res1,
        res_2: &'a mut Res2,
    }

    impl<'a> SystemArgs<'a> for Args<'a> {
        fn required_resources() -> impl Iterator<Item = Request<TypeId>> {
            [
                Request::Read(TypeId::of::<Res1>()),
                Request::Write(TypeId::of::<Res2>()),
            ]
            .into_iter()
        }

        fn from_resources(
            mut resources: impl Iterator<Item = Access<'a, dyn Any + Send + Sync>>,
        ) -> Self {
            Self {
                res_1: resources
                    .next()
                    .expect("incorrect number of resources")
                    .downcast_ref(),
                res_2: resources
                    .next()
                    .expect("incorrect number of resources")
                    .downcast_mut(),
            }
        }
    }

    let mut world = World::new();

    world.add(Res1);
    world.add(Res2);

    let Args { res_1, res_2 } = world.get_args::<Sys>();

    let _ = res_1;
    let _ = res_2;
}

#[test]
fn test_negative_conflict() {
    struct Res1;

    struct Sys;

    impl System for Sys {
        type Args<'a> = Args<'a>;
    }

    struct Args<'a> {
        res_1: &'a Res1,
        res_2: &'a mut Res1,
    }

    impl<'a> SystemArgs<'a> for Args<'a> {
        fn required_resources() -> impl Iterator<Item = Request<TypeId>> {
            [
                Request::Read(TypeId::of::<Res1>()),
                Request::Write(TypeId::of::<Res1>()),
            ]
            .into_iter()
        }

        fn from_resources(
            mut resources: impl Iterator<Item = Access<'a, dyn Any + Send + Sync>>,
        ) -> Self {
            Self {
                res_1: resources
                    .next()
                    .expect("incorrect number of resources")
                    .downcast_ref(),
                res_2: resources
                    .next()
                    .expect("incorrect number of resources")
                    .downcast_mut(),
            }
        }
    }

    let res = std::panic::catch_unwind(move || {
        let mut world = World::new();

        world.add(Res1);

        let Args { res_1, res_2 } = world.get_args::<Sys>();

        let (_, _) = (res_1, res_2);
    });

    assert!(res.is_err());
}

#[test]
fn test_negative_missing_resource() {
    struct Res1;
    struct Res2;

    struct Sys;

    impl System for Sys {
        type Args<'a> = Args<'a>;
    }

    #[derive(SystemArgs)]
    struct Args<'a> {
        res_1: &'a Res1,
        res_2: &'a mut Res2,
    }

    let mut world = World::new();

    world.add(Res1);
    world.add(Sys);

    let res = std::panic::catch_unwind(move || {
        let mut world = World::new();

        world.add(Res1);

        let Args { res_1, res_2 } = world.get_args::<Sys>();

        let (_, _) = (res_1, res_2);
    });

    assert!(res.is_err());
}

#[test]
fn test_system_tuples() {
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

    world.add(Res1);
    world.add(Res2);
    world.add(Res3);
    world.add(Res4);
    world.add(Res5);
    world.add(Res6);

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

#[test]
fn test_system_as_arg() {
    struct Res1;
    struct Res2;

    struct Sys;

    impl System for Sys {
        type Args<'a> = Args<'a>;
    }

    #[derive(SystemArgs)]
    struct Args<'a> {
        sys: &'a mut Sys,
        res_1: &'a Res1,
        res_2: &'a mut Res2,
    }

    let mut world = World::new();

    world.add(Res1);
    world.add(Res2);
    world.add(Sys);

    let Args { sys, res_1, res_2 } = world.get_args::<Sys>();

    let (_, _, _) = (sys, res_1, res_2);
}
