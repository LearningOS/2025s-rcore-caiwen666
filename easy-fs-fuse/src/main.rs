use clap::{App, Arg};
use easy_fs::{BlockDevice, EasyFileSystem};
use std::fs::{read_dir, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::Arc;
use std::sync::Mutex;

const BLOCK_SZ: usize = 512;

struct BlockFile(Mutex<File>);

impl BlockDevice for BlockFile {
    fn read_block(&self, block_id: usize, buf: &mut [u8]) {
        let mut file = self.0.lock().unwrap();
        file.seek(SeekFrom::Start((block_id * BLOCK_SZ) as u64))
            .expect("Error when seeking!");
        assert_eq!(file.read(buf).unwrap(), BLOCK_SZ, "Not a complete block!");
    }

    fn write_block(&self, block_id: usize, buf: &[u8]) {
        let mut file = self.0.lock().unwrap();
        file.seek(SeekFrom::Start((block_id * BLOCK_SZ) as u64))
            .expect("Error when seeking!");
        assert_eq!(file.write(buf).unwrap(), BLOCK_SZ, "Not a complete block!");
    }
}

fn main() {
    easy_fs_pack().expect("Error when packing easy-fs!");
}

fn easy_fs_pack() -> std::io::Result<()> {
    let matches = App::new("EasyFileSystem packer")
        .arg(
            Arg::with_name("source")
                .short("s")
                .long("source")
                .takes_value(true)
                .help("Executable source dir(with backslash)"),
        )
        .arg(
            Arg::with_name("target")
                .short("t")
                .long("target")
                .takes_value(true)
                .help("Executable target dir(with backslash)"),
        )
        .get_matches();
    let src_path = matches.value_of("source").unwrap();
    let target_path = matches.value_of("target").unwrap();
    println!("src_path = {}\ntarget_path = {}", src_path, target_path);
    let block_file = Arc::new(BlockFile(Mutex::new({
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(format!("{}{}", target_path, "fs.img"))?;
        f.set_len(16 * 2048 * 512).unwrap();
        f
    })));
    // 16MiB, at most 4095 files
    let efs = EasyFileSystem::create(block_file, 16 * 2048, 1);
    let root_inode = Arc::new(EasyFileSystem::root_inode(&efs));
    let apps: Vec<_> = read_dir(src_path)
        .unwrap()
        .into_iter()
        .map(|dir_entry| {
            let mut name_with_ext = dir_entry.unwrap().file_name().into_string().unwrap();
            name_with_ext.drain(name_with_ext.find('.').unwrap()..name_with_ext.len());
            name_with_ext
        })
        .collect();
    for app in apps {
        // load app data from host file system
        let mut host_file = File::open(format!("{}{}", target_path, app)).unwrap();
        let mut all_data: Vec<u8> = Vec::new();
        host_file.read_to_end(&mut all_data).unwrap();
        // create a file in easy-fs
        let inode = root_inode.create(app.as_str()).unwrap();
        // write data to easy-fs
        inode.write_at(0, all_data.as_slice());
    }
    // list apps
    // for app in root_inode.ls() {
    //     println!("{}", app);
    // }
    Ok(())
}

#[test]
fn efs_test() -> std::io::Result<()> {
    let block_file = Arc::new(BlockFile(Mutex::new({
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("target/fs.img")?;
        f.set_len(8192 * 512).unwrap();
        f
    })));
    EasyFileSystem::create(block_file.clone(), 4096, 1);
    let efs = EasyFileSystem::open(block_file.clone());
    let root_inode = EasyFileSystem::root_inode(&efs);
    root_inode.create("filea");
    root_inode.create("fileb");
    for name in root_inode.ls() {
        println!("{}", name);
    }
    let filea = root_inode.find("filea").unwrap();
    let greet_str = "Hello, world!";
    filea.write_at(0, greet_str.as_bytes());
    //let mut buffer = [0u8; 512];
    let mut buffer = [0u8; 233];
    let len = filea.read_at(0, &mut buffer);
    assert_eq!(greet_str, core::str::from_utf8(&buffer[..len]).unwrap(),);

    let mut random_str_test = |len: usize| {
        filea.clear();
        assert_eq!(filea.read_at(0, &mut buffer), 0,);
        let mut str = String::new();
        use rand;
        // random digit
        for _ in 0..len {
            str.push(char::from('0' as u8 + rand::random::<u8>() % 10));
        }
        filea.write_at(0, str.as_bytes());
        let mut read_buffer = [0u8; 127];
        let mut offset = 0usize;
        let mut read_str = String::new();
        loop {
            let len = filea.read_at(offset, &mut read_buffer);
            if len == 0 {
                break;
            }
            offset += len;
            read_str.push_str(core::str::from_utf8(&read_buffer[..len]).unwrap());
        }
        assert_eq!(str, read_str);
    };

    random_str_test(4 * BLOCK_SZ);
    random_str_test(8 * BLOCK_SZ + BLOCK_SZ / 2);
    random_str_test(100 * BLOCK_SZ);
    random_str_test(70 * BLOCK_SZ + BLOCK_SZ / 7);
    random_str_test((12 + 128) * BLOCK_SZ);
    random_str_test(400 * BLOCK_SZ);
    random_str_test(1000 * BLOCK_SZ);
    random_str_test(2000 * BLOCK_SZ);

    Ok(())
}

use rand::distributions::{Alphanumeric, DistString};
use rand::Rng;
fn generate_random_string(length: usize) -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), length)
}

#[test]
fn link_test() -> std::io::Result<()> {
    let block_file = Arc::new(BlockFile(Mutex::new({
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("target/fs.img")
            .unwrap();
        f.set_len(8192 * 512).unwrap();
        f
    })));
    EasyFileSystem::create(
        block_file.clone(),
        4096,
        1
    );
    let efs = EasyFileSystem::open(block_file.clone());
    let root_inode = EasyFileSystem::root_inode(&efs);
    root_inode.clear();
    assert_eq!(root_inode.ls(), vec![] as Vec<String>);
    let mut list = Vec::new();
    let mut link_count = Vec::new();
    let mut link_target = Vec::new();
    let mut link_list = Vec::new();
    for i in 0..=17 {
        let name = generate_random_string(i + 1);
        list.push(name.clone());
        let file = root_inode.create(name.as_str()).unwrap();
        file.write_at(0, name.repeat(1000).as_bytes());
        link_count.push(1);
    }
    assert_eq!(root_inode.ls(), list);
    let mut rng = rand::thread_rng();
    for i in 0..=17 {
        let link_to = rng.gen_range(0..=17);
        link_target.push(link_to);
        let target = root_inode.find(list[link_to].as_str()).unwrap();
        let name = generate_random_string(i + 1);
        link_list.push(name.clone());
        root_inode.link(name.as_str(), target.get_inode_id());
        link_count[link_to] += 1;
        assert_eq!(root_inode.find(name.as_str()).unwrap().get_inode_id(), target.get_inode_id());
    }
    assert_eq!(root_inode.ls(), list.iter().chain(link_list.iter()).cloned().collect::<Vec<String>>());
    for (i, target) in link_target.iter().enumerate() {
        println!("{:?}", link_count);
        let name = link_list[i].clone();
        let content = list[*target].clone();
        let file = root_inode.find(name.as_str()).unwrap();
        let base = root_inode.find(content.as_str()).unwrap();
        assert_eq!(file.get_inode_id(), base.get_inode_id());
        println!("{}", base.get_inode_id());
        assert_eq!(file.nlink(), link_count[*target] as u32);
        let mut buffer = vec![0u8; content.repeat(999).as_bytes().len()];
        file.read_at(content.as_bytes().len(), buffer.as_mut_slice());
        let sa = content.repeat(999);
        let sb = String::from_utf8(buffer).unwrap();
        assert_eq!(sa, sb);
        root_inode.remove(name.as_str());
        link_count[*target] -= 1;
        println!("{:?}", link_count);
        println!("{:?}", list.iter().map(|v| {
            root_inode.find(v.as_str()).unwrap().nlink() as i32
        }).collect::<Vec<i32>>());
        assert_eq!(link_count, list.iter().map(|v| {
            root_inode.find(v.as_str()).unwrap().nlink() as i32
        }).collect::<Vec<i32>>());
    }
    assert_eq!(root_inode.ls(), list);
    for i in &list {
        root_inode.remove(i.as_str());
    }
    assert_eq!(root_inode.ls(), Vec::<String>::new());
    Ok(())
}