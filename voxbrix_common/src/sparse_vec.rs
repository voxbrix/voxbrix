use std::{
    collections::VecDeque,
    iter::Iterator,
    mem,
};

pub struct SparseVec<T> {
    values: Vec<Option<T>>,
    free_ids: VecDeque<usize>,
}

impl<T> SparseVec<T> {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            free_ids: VecDeque::new(),
        }
    }

    pub fn get(&self, id: usize) -> Option<&T> {
        self.values.get(id)?.as_ref()
    }

    pub fn get_mut(&mut self, id: usize) -> Option<&mut T> {
        self.values.get_mut(id)?.as_mut()
    }

    pub fn insert(&mut self, id: usize, new: T) -> Option<T> {
        if self.values.len() > id {
            mem::replace(self.values.get_mut(id).unwrap(), Some(new))
        } else {
            self.values.resize_with(id, || None);
            self.values.push(Some(new));
            None
        }
    }

    pub fn push(&mut self, value: T) -> usize {
        loop {
            match self.free_ids.pop_front() {
                Some(id) => {
                    match self.values.get_mut(id) {
                        Some(value_opt) => {
                            *value_opt = Some(value);
                            return id;
                        },
                        None => continue,
                    };
                },
                None => {
                    self.values.push(Some(value));
                    return self.values.len() - 1;
                },
            }
        }
    }

    pub fn remove(&mut self, id: usize) -> Option<T> {
        let res = mem::replace(self.values.get_mut(id)?, None);

        if res.is_some() {
            self.free_ids.push_back(id);

            loop {
                match self.values.last() {
                    Some(None) => {
                        self.values.pop();
                    },
                    _ => break,
                }
            }
        }

        res
    }

    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.values.iter().filter_map(|v| Some(v.as_ref()?))
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, &T)> {
        self.values
            .iter()
            .enumerate()
            .filter_map(|(i, v)| Some((i, v.as_ref()?)))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (usize, &mut T)> {
        self.values
            .iter_mut()
            .enumerate()
            .filter_map(|(i, v)| Some((i, v.as_mut()?)))
    }
}
