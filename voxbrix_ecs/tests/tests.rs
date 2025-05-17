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

    world.insert(Res1);
    world.insert(Res2);

    let Args { res_1, res_2 } = world.get_args::<Sys>();

    let _ = res_1;
    let _ = res_2;
}

#[test]
fn test_negative() {
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

        world.insert(Res1);

        let Args { res_1, res_2 } = world.get_args::<Sys>();

        let (_, _) = (res_1, res_2);
    });

    assert!(res.is_err());
}
