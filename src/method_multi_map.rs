use crate::request::{METHODS, Method};
use std::mem;

#[derive(Debug, Clone, Copy)]
pub struct MethodMultiMap<T: Sized> {
    indicies: [Option<usize>; mem::variant_count::<Method>()],
    values: [Option<T>; mem::variant_count::<Method>()],
}

impl<T: Sized> Default for MethodMultiMap<T> {
    fn default() -> Self {
        Self {
            indicies: Default::default(),
            values: Default::default(),
        }
    }
}

impl<T: Sized> MethodMultiMap<T> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn map(&self, method: &Method) -> Option<&T> {
        let index = (*self.indicies.get(method.index())?)?;
        self.values
            .get(index)
            .and_then(|maybe_inner| maybe_inner.as_ref())
    }

    pub fn map_mut(&mut self, method: &Method) -> Option<&mut T> {
        let index = (*self.indicies.get(method.index())?)?;
        self.values
            .get_mut(index)
            .and_then(|maybe_inner| maybe_inner.as_mut())
    }

    pub fn iter_mapped_methods(&self) -> impl Iterator<Item = Method> {
        METHODS
            .iter()
            .filter(|method| self.indicies.get(method.index()).is_some())
            .copied()
    }

    fn unmap_method(&mut self, method: &Method) {
        let index = method.index();
        if let Some(removed) = self.indicies[index].take()
            && removed == index
        {
            let new_index = self
                .indicies
                .iter()
                .enumerate()
                .filter(|(_ref_index, other_ref)| other_ref.is_some_and(|mapped| mapped == index))
                .map(|(ref_index, _other_ref)| ref_index)
                .min();
            if let Some(new_index) = new_index {
                self.values.swap(index, new_index);
            }
            for mapping in self
                .indicies
                .iter_mut()
                .filter(|other_ref| other_ref.is_some_and(|mapped| mapped == index))
            {
                *mapping = new_index;
            }
            self.values[index] = None;
        }
    }

    pub fn insert(&mut self, value: T, methods: &[Method]) {
        if methods.is_empty() {
            return;
        }
        for method in methods {
            self.unmap_method(method);
        }
        let indicies = methods.iter().map(|method| method.index());
        let min_index = indicies.clone().min().expect("Non-empty iterator");
        let _ = self.values[min_index].insert(value);
        for index in indicies {
            let _ = self.indicies[index].insert(min_index);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, hint::black_box};

    use rand::{RngExt, rng, seq::IndexedRandom};

    use super::*;

    fn assert_single_index(values: &[Option<usize>], target_index: usize, value: usize) {
        for (index, mapped) in values.iter().enumerate() {
            match index {
                i if i == target_index => assert_eq!(mapped.unwrap(), value),
                _ => assert!(mapped.is_none()),
            }
        }
    }

    #[test]
    fn create_empty() {
        let empty: MethodMultiMap<u8> = MethodMultiMap::new();
        for index in empty.indicies {
            assert!(index.is_none());
        }
        for value in empty.values {
            assert!(value.is_none());
        }
        for method in &METHODS {
            assert!(empty.map(method).is_none());
        }
    }

    #[test]
    fn map_one_to_one() {
        let mut mapper: MethodMultiMap<usize> = MethodMultiMap::new();
        mapper.insert(12345, &[Method::Get]);
        assert_single_index(&mapper.indicies, Method::Get.index(), Method::Get.index());
        assert_single_index(&mapper.values, Method::Get.index(), 12345);
        for method in &METHODS {
            match method {
                Method::Get => assert_eq!(*mapper.map(method).unwrap(), 12345),
                _ => assert!(mapper.map(method).is_none()),
            }
        }
    }

    #[test]
    fn many_to_one() {
        let mut mapper: MethodMultiMap<usize> = MethodMultiMap::new();
        let method_slice = &METHODS[1..8];
        let indicies: Vec<usize> = method_slice.iter().map(|method| method.index()).collect();
        let min_index = *indicies.iter().min().unwrap();
        mapper.insert(56789, method_slice);
        for (index, mapping) in mapper.indicies.iter().enumerate() {
            match index {
                i if indicies.contains(&i) => assert_eq!(mapping.unwrap(), min_index),
                _ => assert!(mapping.is_none()),
            }
        }
        assert_single_index(&mapper.values, min_index, 56789);
        for method in METHODS {
            let mapped = mapper.map(&method);
            if method_slice.contains(&method) {
                assert_eq!(*mapped.unwrap(), 56789);
            } else {
                assert!(mapped.is_none());
            }
        }
    }

    #[test]
    fn all_mapped() {
        let mut mapper: MethodMultiMap<usize> = MethodMultiMap::new();
        for (value, key) in METHODS
            .iter()
            .enumerate()
            .map(|(index, value)| (index + 100, value))
        {
            mapper.insert(value, &[*key]);
        }
        for (index, mapping) in mapper.indicies.iter().enumerate() {
            assert_eq!(mapping.unwrap(), index);
        }
        for (index, value) in mapper.values.iter().enumerate() {
            assert_eq!(value.unwrap(), index + 100);
        }
    }

    #[test]
    fn mixed_mappings() {
        let mut mapper: MethodMultiMap<usize> = MethodMultiMap::new();
        let method_slice = &METHODS[2..7];
        let slice_indicies: Vec<usize> = method_slice.iter().map(|method| method.index()).collect();
        let min_index = *slice_indicies.iter().min().unwrap();
        for (index, method) in METHODS.iter().enumerate() {
            if !method_slice.contains(method) {
                mapper.insert(index + 100, &[*method]);
            }
        }
        mapper.insert(12345, method_slice);
        for (index, mapping) in mapper.indicies.iter().enumerate() {
            assert_eq!(
                mapping.unwrap(),
                if slice_indicies.contains(&index) {
                    min_index
                } else {
                    index
                }
            );
        }
        for (index, value) in mapper.values.iter().enumerate() {
            match index {
                i if slice_indicies.contains(&i) && i == min_index => {
                    assert_eq!(value.unwrap(), 12345)
                }
                i if slice_indicies.contains(&i) => assert!(value.is_none()),
                _ => assert_eq!(value.unwrap(), index + 100),
            }
        }
        for method in &METHODS {
            let index = method.index();
            let mapped = *mapper.map(method).unwrap();
            assert_eq!(
                mapped,
                if slice_indicies.contains(&index) {
                    12345
                } else {
                    index + 100
                }
            );
        }
    }

    #[allow(clippy::unit_arg, reason = "Black box")]
    #[test]
    fn replace_single() {
        let mut mapper: MethodMultiMap<usize> = MethodMultiMap::new();
        black_box(mapper.insert(black_box(123), black_box(&[Method::Get])));
        black_box(mapper.insert(black_box(456), black_box(&[Method::Get])));
        assert_single_index(&mapper.indicies, Method::Get.index(), Method::Get.index());
        assert_single_index(&mapper.values, Method::Get.index(), 456);
    }

    #[allow(clippy::unit_arg, reason = "Black box")]
    #[test]
    fn replace_last() {
        let mut mapper: MethodMultiMap<usize> = MethodMultiMap::new();
        let method_slice = &METHODS[0..3];
        let slice_indicies: Vec<usize> = method_slice.iter().map(|method| method.index()).collect();
        let min_index = *slice_indicies.iter().min().unwrap();
        black_box(mapper.insert(black_box(123), black_box(method_slice)));
        black_box(mapper.insert(black_box(456), black_box(&[*method_slice.last().unwrap()])));

        for method in METHODS {
            match method.index() {
                index
                    if slice_indicies.contains(&index)
                        && *slice_indicies.last().unwrap() == index =>
                {
                    assert_eq!(mapper.indicies[index].unwrap(), index);
                    assert_eq!(mapper.values[index].unwrap(), 456);
                    assert_eq!(*mapper.map(&method).unwrap(), 456);
                }
                index if slice_indicies.contains(&index) => {
                    assert_eq!(mapper.indicies[index].unwrap(), min_index);
                    assert_eq!(mapper.values[min_index].unwrap(), 123);
                    assert_eq!(*mapper.map(&method).unwrap(), 123);
                }
                index => {
                    assert!(mapper.indicies[index].is_none());
                    assert!(mapper.values[index].is_none());
                    assert!(mapper.map(&method).is_none());
                }
            }
        }
    }

    #[allow(clippy::unit_arg, reason = "Black box")]
    #[test]
    fn replace_middle() {
        let mut mapper: MethodMultiMap<usize> = MethodMultiMap::new();
        let method_slice = &METHODS[0..3];
        let slice_indicies: Vec<usize> = method_slice.iter().map(|method| method.index()).collect();
        let min_index = *slice_indicies.iter().min().unwrap();
        black_box(mapper.insert(black_box(123), black_box(method_slice)));
        black_box(mapper.insert(black_box(456), black_box(&[*method_slice.get(1).unwrap()])));

        for method in METHODS {
            match method.index() {
                index
                    if slice_indicies.contains(&index)
                        && *slice_indicies.get(1).unwrap() == index =>
                {
                    assert_eq!(mapper.indicies[index].unwrap(), index);
                    assert_eq!(mapper.values[index].unwrap(), 456);
                    assert_eq!(*mapper.map(&method).unwrap(), 456);
                }
                index if slice_indicies.contains(&index) => {
                    assert_eq!(mapper.indicies[index].unwrap(), min_index);
                    assert_eq!(mapper.values[min_index].unwrap(), 123);
                    assert_eq!(*mapper.map(&method).unwrap(), 123);
                }
                index => {
                    assert!(mapper.indicies[index].is_none());
                    assert!(mapper.values[index].is_none());
                    assert!(mapper.map(&method).is_none());
                }
            }
        }
    }

    #[allow(clippy::unit_arg, reason = "Black box")]
    #[test]
    fn replace_first() {
        let mut mapper: MethodMultiMap<usize> = MethodMultiMap::new();
        let method_slice = &METHODS[0..3];
        let slice_indicies: Vec<usize> = method_slice.iter().map(|method| method.index()).collect();
        let min_index = *slice_indicies.iter().skip(1).min().unwrap();
        black_box(mapper.insert(black_box(123), black_box(method_slice)));
        black_box(mapper.insert(black_box(456), black_box(&[*method_slice.first().unwrap()])));

        for method in METHODS {
            match method.index() {
                index
                    if slice_indicies.contains(&index)
                        && method_slice.first().unwrap().index() == index =>
                {
                    assert_eq!(mapper.indicies[index].unwrap(), index);
                    assert_eq!(mapper.values[index].unwrap(), 456);
                    assert_eq!(*mapper.map(&method).unwrap(), 456);
                }
                index if slice_indicies.contains(&index) => {
                    assert_eq!(mapper.indicies[index].unwrap(), min_index);
                    assert_eq!(mapper.values[min_index].unwrap(), 123);
                    assert_eq!(*mapper.map(&method).unwrap(), 123);
                }
                index => {
                    assert!(mapper.indicies[index].is_none());
                    assert!(mapper.values[index].is_none());
                    assert!(mapper.map(&method).is_none());
                }
            }
        }
    }

    #[test]
    fn random_bulk() {
        let mut mapper: MethodMultiMap<u64> = MethodMultiMap::new();
        let mut heap_mapper: HashMap<Method, u64> = HashMap::new();
        let mut rng = rng();
        for _ in 0..10000 {
            let num_methods = rng.random_range(0..METHODS.len());
            let methods: Vec<Method> = METHODS.sample(&mut rng, num_methods).copied().collect();
            let value = rng.random();
            mapper.insert(value, &methods);
            for method in methods {
                heap_mapper.insert(method, value);
            }
            for (key, &value) in &heap_mapper {
                assert_eq!(value, *mapper.map(key).unwrap());
            }
        }
    }
}
