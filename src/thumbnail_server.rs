extern crate iron;
extern crate staticfile;
extern crate mount;

pub fn serve(path: &std::path::Path, listen_address: &str) {
    let mut mount = mount::Mount::new();

    mount.mount("/", staticfile::Static::new(path));

    iron::Iron::new(mount).http(listen_address).unwrap();
}

