/*use std::{
    collections::HashMap,
    any::Any,
};

trait System {
    fn run(&mut self, components: &[&(dyn Any)], components_mut: &[&mut (dyn Any)]);
}

struct SystemDescription {
    system: Box<dyn System>,
    components: Vec<String>,
    components_mut: Vec<String>,
}

pub struct Manager {
    systems: Vec<SystemDescription>,
    components: HashMap<String, Box<dyn Any>>,
}

impl Manager {
    pub fn run(&self) {
        loop {
            for system in self.systems {
                
            }
        }
    }
}*/

/*pub async fn unblock<T, F>(mut param: T, mut func: F) -> T
where
    T: 'static + Send,
    F: 'static + FnMut(&mut T) + Send,
{
    blocking::unblock(move ||  {
        func(&mut param);

        param
    }).await
}*/
