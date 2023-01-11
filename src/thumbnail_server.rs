extern crate iron;
extern crate staticfile;
extern crate mount;

pub fn serve(path: &std::path::Path) {
    let mut mount = mount::Mount::new();

    mount.mount("/", staticfile::Static::new(path));

    iron::Iron::new(mount).http("127.0.0.1:3000").unwrap();
}

