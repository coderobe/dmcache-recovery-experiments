use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

use clap::{App, Arg, SubCommand};
use memmap2::{MmapMut, MmapOptions};
use sha1::{Digest, Sha1};

const HASH_BYTES: usize = 20;
const BLOCK_SIZE: usize = 8 * 1024;
const MMAP_BLOCK_SIZE: usize = 1024 * 1024 * 128;

#[derive(Debug)]
struct MappedFile {
    mmap: MmapMut,
    size: usize,
}

impl MappedFile {
    fn open(path: &Path, write: bool) -> io::Result<Self> {
        let file = if write {
            OpenOptions::new().read(true).write(false/*coderobe: hehe*/).open(path)?
        } else {
            OpenOptions::new().read(true).write(false).open(path)?
        };
        let size = file.metadata()?.len() as usize;
        let mmap = unsafe { MmapOptions::new().map_mut(&file)? };
        Ok(Self { mmap, size })
    }

    fn create(path: &Path, size: usize) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.set_len(size as u64)?;
        let mmap = unsafe { MmapOptions::new().map_mut(&file)? };
        Ok(Self { mmap, size })
    }

    fn size(&self) -> usize {
        self.size
    }

    fn slice(&self, offset: usize, len: usize) -> &[u8] {
        &self.mmap[offset..offset + len]
    }

    fn slice_mut(&mut self, offset: usize, len: usize) -> &mut [u8] {
        &mut self.mmap[offset..offset + len]
    }
}

fn hash_block(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha1::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

fn log_status(current: usize, total: usize, unit: &str, newline: bool) {
    let percentage = 100.0 * (current as f64 / total as f64);
    eprint!(
        "{:5.1} % - {:} of {:} {}{}",
        percentage,
        current,
        total,
        unit,
        if newline { "\n" } else { "\r" }
    );
}

fn log_complete(total: usize, unit: &str) {
    eprint!("100.0 % - {:} of {:} {}\r", total, total, unit);
}

fn collect(index_path: &Path, device_path: &Path) -> io::Result<()> {
    let device = MappedFile::open(device_path, false)?;
    let device_size = device.size();
    let block_count = (device_size + BLOCK_SIZE - 1) / BLOCK_SIZE;
    let index_block_count = (block_count + BLOCK_SIZE - 1) / BLOCK_SIZE;

    let mut index_file = MappedFile::create(index_path, index_block_count * BLOCK_SIZE)?;

    let mut index_block = 0;
    let mut index_entry = 0;

    for offset in (0..device_size).step_by(BLOCK_SIZE) {
        if offset % (BLOCK_SIZE * 10240) == 0 {
            log_status(offset, device_size, "bytes", false);
        }

        let digest = hash_block(&device.slice(offset, BLOCK_SIZE));
        let index_offset = index_block * BLOCK_SIZE + index_entry * HASH_BYTES;

        index_file.slice_mut(index_offset, HASH_BYTES).copy_from_slice(&digest);
        index_entry += 1;

        if index_entry >= BLOCK_SIZE {
            index_block += 1;
            index_entry = 0;
        }
    }
    log_complete(device_size, "bytes");
    Ok(())
}

fn find(index_path: &Path, cache_device_path: &Path, cache_block_size: usize) -> io::Result<()> {
    let index_file = MappedFile::open(index_path, false)?;
    let device_size = index_file.size();
    let mut index = HashMap::new();

    for block_offset in (0..device_size).step_by(BLOCK_SIZE) {
        let block_bytes = &index_file.slice(block_offset, BLOCK_SIZE);
        for entry in (0..BLOCK_SIZE).step_by(HASH_BYTES) {
            let digest = &block_bytes[entry..entry + HASH_BYTES];
            let offset = block_offset + entry;
            index.entry(digest.to_vec()).or_insert_with(Vec::new).push(offset);
        }
    }
    log_complete(device_size, "bytes");

    let cache_device = MappedFile::open(cache_device_path, false)?;
    let cache_block_size = 512 * cache_block_size;
    let cache_total_blocks = cache_device.size() / cache_block_size;

    for cache_block in 0..cache_total_blocks {
        log_status(cache_block, cache_total_blocks, "blocks", true);
        let mut matches = HashMap::new();
        let mut fake_matches = 0;

        for fs_block in 0..(cache_block_size / BLOCK_SIZE) {
            let offset = cache_block * cache_block_size + fs_block * BLOCK_SIZE;
            let digest = hash_block(&cache_device.slice(offset, BLOCK_SIZE));

            if let Some(matches_vec) = index.get(&digest) {
                for match_offset in matches_vec {
                    let origin_fs_block = match_offset / BLOCK_SIZE;
                    let origin_cache_block = match_offset / cache_block_size;
                    let origin_local_fs_block = origin_fs_block % (cache_block_size / BLOCK_SIZE);

                    if origin_local_fs_block != fs_block {
                        fake_matches += 1;
                        continue;
                    }
                    *matches.entry(origin_cache_block).or_insert(0) += 1;
                }
            }
        }

        let mut first = true;
        let mut match_vec: Vec<_> = matches.iter().collect();
        match_vec.sort_by_key(|&(_, count)| std::cmp::Reverse(count));
        for (origin_cache_block, count) in match_vec {
            println!(
                "{}{} -> {} ({:.3}% match)",
                if first { "" } else { "#" },
                cache_block,
                origin_cache_block,
                *count as f64 / (cache_block_size / BLOCK_SIZE) as f64 * 100.0
            );
            first = false;
        }

        if fake_matches != 0 {
            println!("#{} fake matches", fake_matches);
        }
    }
    log_complete(cache_total_blocks, "blocks");
    Ok(())
}

fn main() -> io::Result<()> {
    let matches = App::new("cache_guess")
        .subcommand(
            SubCommand::with_name("collect")
                .arg(Arg::with_name("index").required(true))
                .arg(Arg::with_name("device").required(true)),
        )
        .subcommand(
            SubCommand::with_name("find")
                .arg(Arg::with_name("index").required(true))
                .arg(Arg::with_name("cache_device").required(true))
                .arg(
                    Arg::with_name("cache-block-size")
                        .long("cache-block-size")
                        .default_value("512")
                        .help("In sectors (512 bytes)"),
                ),
        )
        .get_matches();

    match matches.subcommand() {
        ("collect", Some(sub_m)) => {
            let index_path = Path::new(sub_m.value_of("index").unwrap());
            let device_path = Path::new(sub_m.value_of("device").unwrap());
            collect(index_path, device_path)
        }
        ("find", Some(sub_m)) => {
            let index_path = Path::new(sub_m.value_of("index").unwrap());
            let cache_device_path = Path::new(sub_m.value_of("cache_device").unwrap());
            let cache_block_size = sub_m.value_of("cache-block-size").unwrap().parse::<usize>().unwrap();
            find(index_path, cache_device_path, cache_block_size)
        }
        _ => Ok(()),
    }
}
