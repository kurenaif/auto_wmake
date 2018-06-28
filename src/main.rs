extern crate regex;

use std::io::Read;
use std::path::Path;
use std::fs::{self, metadata, File};
use std::error::Error;
use std::env;
use std::ffi::OsStr;
use std::collections::LinkedList;
use std::collections::{HashMap, HashSet, VecDeque};
use std::process::Command;

use regex::Regex;

fn get_make_target(dir_name: &Path) -> Option<(String, String)> {
    let path = Path::new(dir_name).join("Make/files");
    let display = path.display();

    let mut file = match File::open(&path) {
        Err(why) => panic!("couldn't open {}: {}", display, Error::description(&why)),
        Ok(file) => file,
    };

    let mut s = String::new();
    let contents = match file.read_to_string(&mut s) {
        Err(why) => panic!("couldn't read {}: {}", display, Error::description(&why)),
        Ok(_) => s,
    };

    for line in contents.as_str().split('\n') {
        let line = str::replace(line, " ", "");
        let mut split_line = line.split('=');
        let first = split_line.next();
        let second = split_line.next();
        if second != None {
            let first = first.unwrap().to_string();
            let second: String = Path::new(second.unwrap())
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
                .chars()
                .skip(3)
                .collect();
            if first == "LIB" || first == "EXE" {
                return Some((first, second));
            }
        }
    }
    None
}

fn get_dependencies(dir_name: &Path) -> Vec<String> {
    let mut res: Vec<String> = Vec::new();

    let path = Path::new(dir_name).join("Make/options");
    let display = path.display();

    let mut file = match File::open(&path) {
        Err(why) => panic!("couldn't open {}: {}", display, Error::description(&why)),
        Ok(file) => file,
    };

    let mut s = String::new();
    let contents = match file.read_to_string(&mut s) {
        Err(why) => panic!("couldn't read {}: {}", display, Error::description(&why)),
        Ok(_) => s,
    };

    let re = Regex::new(r"\\(?s:.)").unwrap();
    let contents = re.replace_all(&contents, "");

    for line in contents.split('\n') {
        let mut split_line = line.split('=').clone();
        let dependency_type = str::replace(split_line.next().unwrap(), " ", "");
        let second = split_line.next();
        if second != None {
            let second = second.unwrap();
            if dependency_type == "EXE_LIBS" || dependency_type == "LIB_LIBS" {
                for file in second.split_whitespace() {
                    let string_file: String = file.to_string().chars().skip(2).collect();
                    res.push(string_file)
                }
            }
        }
    }
    res
}

fn walk_dir(dir: &Path) -> LinkedList<String> {
    let mut res: LinkedList<String> = LinkedList::new();

    match fs::read_dir(dir) {
        Err(why) => println!("! {:?}", why.kind()),
        Ok(paths) => for path in paths {
            let path = path.unwrap();
            if path.file_name() == OsStr::new("Make") {
                res.push_back(dir.to_str().unwrap().to_string());
                break;
            }
            let nxt = String::from(path.path().to_str().unwrap());
            let nxt = Path::new(&nxt);
            if metadata(nxt).unwrap().is_dir() {
                res.append(&mut walk_dir(nxt));
            }
        },
    }

    res
}

fn get_edges(root_dir: &Path, memo: &mut HashSet<String>) -> LinkedList<(String, String)> {
    let mut res: LinkedList<(String, String)> = LinkedList::new();

    let src_dir = env::var("FOAM_SRC").unwrap();
    let make_candidate_dirs = walk_dir(Path::new(&src_dir));
    let mut library_dirs = HashMap::new();
    for dir in make_candidate_dirs {
        library_dirs.insert(get_make_target(Path::new(&dir)).unwrap().1, dir);
    }
    for dir in get_dependencies(root_dir) {
        match library_dirs.get(&dir) {
            Some(nxt) => {
                res.push_back((root_dir.to_string_lossy().into_owned(), nxt.to_string()));
                if !memo.contains(&nxt.to_string()) {
                    res.append(&mut get_edges(Path::new(nxt), memo));
                }
                memo.insert(nxt.to_string());
            }
            None => continue,
        }
    }

    res
}

fn get_zero_in_degree(in_degree: &HashMap<String, i32>) -> VecDeque<String> {
    let mut queue = VecDeque::new();
    for (node, degree) in in_degree {
        if *degree == 0 {
            queue.push_back(node.to_string());
        }
    }
    queue
}

fn main() {
    let root_path = "/home/ko/OpenFOAM/OpenFOAM-dev/applications/solvers/incompressible/pimpleFoam";
    let edges = get_edges(
        Path::new(
            root_path,
        ),
        &mut HashSet::new(),
    );
    let mut graph : HashMap<String, Vec<String>> = HashMap::new();
    let mut in_degree : HashMap<String, i32> = HashMap::new();

    for edge in edges {
        if !graph.contains_key(&edge.0) {
            graph.insert(edge.0.clone(), Vec::new());
        }
        if !graph.contains_key(&edge.1) {
            graph.insert(edge.1.clone(), Vec::new());
        }
        if !in_degree.contains_key(&edge.0) {
            in_degree.insert(edge.0.clone(), 0);
        }
        if !in_degree.contains_key(&edge.1) {
            in_degree.insert(edge.1.clone(), 0);
        }
        *in_degree.get_mut(&edge.0).unwrap() += 1;
        graph.get_mut(&edge.1).unwrap().push(edge.0);
    }

    let mut queue = get_zero_in_degree(&in_degree);

    while let Some(target) = queue.pop_front() {
        println!("{}", target);
        let out_command = Command::new("wmake")
            .arg("-j4")
            .current_dir(&target)
            .output()
            .expect("failed");
        let out_string = out_command.stdout;
        println!("{}", std::str::from_utf8(&out_string).unwrap());
        let nexts = graph.get_mut(&target).unwrap();
        for nxt in nexts {
            *in_degree.get_mut(nxt).unwrap() -= 1;
            if in_degree.get(nxt).unwrap() == &0 {
                queue.push_back(nxt.to_string());
            }
        }
    }
}
