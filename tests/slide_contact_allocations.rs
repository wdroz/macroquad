use macroquad::prelude::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountingAllocator;
thread_local! {
    static COUNT: Cell<Option<usize>> = const { Cell::new(None) };
}
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        COUNT.with(|count| {
            if let Some(n) = count.get() {
                count.set(Some(n + 1));
            }
        });
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn both_slide_paths_allocate_nothing_with_stack_contact_storage() {
    let floor = Aabb::from_center_size(vec3(0., -0.5, 0.), vec3(20., 1., 20.));
    let wall = Aabb::from_center_size(vec3(3., 1., 0.), vec3(1., 4., 20.));
    let centers = [vec3(-1., 0.5, 0.), vec3(1., 0.5, 0.)];
    let delta = vec3(4., -0.25, 1.);
    let mut contacts = [None; 16];
    let mut count = 0;
    COUNT.with(|n| n.set(Some(0)));
    let plain = slide_spheres_solids(&centers, 0.5, delta, &[floor, wall], &[], &[], &[]);
    let observed = slide_spheres_solids_with_contacts(
        &centers,
        0.5,
        delta,
        &[floor, wall],
        &[],
        &[],
        &[],
        |event| {
            contacts[count] = Some(event);
            count += 1;
        },
    );
    let allocations = COUNT.with(|n| n.replace(None).unwrap());
    assert_eq!(allocations, 0);
    assert_eq!(plain, observed);
    assert_eq!(count, 2);
}
