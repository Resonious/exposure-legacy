#[macro_use]
extern crate lazy_static;
extern crate regex;

use regex::Regex;
use fnv::{FnvHashMap, FnvHashSet};

use std::thread::{self, JoinHandle};
use std::fmt;
use std::ffi::{CStr, OsStr};
use std::os::raw::{c_char, c_void};
use std::sync::mpsc;
use std::boxed::Box;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::env;
use std::fs::File;
use std::fs;
use std::io::{self, BufRead};
use std::io::prelude::*;

/// Enum corresponding to the tracepoint events we are about
pub enum EventType {
    BCall = 1,
    Class = 2,
    Call = 3,
    Return = 4,
    BReturn = 5,
    End = 6,
}

/// YEAH
pub enum Event {
    // File, line no
    BCall(String, i32),
    // Class name
    Class(String),
    // Class name, method name
    Call(String, String),
}

impl Event {
    pub fn format(&self) -> String {
        match self {
            Event::Call(class, method) => Event::format_call(class, method),
            Event::Class(name)         => Event::sanitized_class_name(&name).to_string(),
            Event::BCall(file, line)   => Event::format_bcall(file, *line)
        }
    }

    fn sanitized_class_name(class_name: &str) -> Cow<str> {
        lazy_static! {
            static ref GENERATED_ID_REGEX: Regex = Regex::new(":0x[\\dA-Fa-f]{16}")
                .unwrap();
        }
        let result = GENERATED_ID_REGEX.replace_all(class_name, ":(generated)");
        let result_str: &str = &result;

        match result_str {
            "NilClass"                 => Cow::from("nil"),
            "FalseClass" | "TrueClass" => Cow::from("Boolean"),
            _                          => result
        }
    }

    fn format_bcall(file: &str, line: i32) -> String {
        let path = Path::new(file);

        let last2: Vec<&OsStr> = path.iter().rev().take(2).collect();

        let mut strings: Vec<String> = last2
            .iter()
            .rev()
            .map(|s| s.to_string_lossy().to_string())
            .collect();

        strings.push(line.to_string());
        strings.join(" ")
    }

    fn format_call(class: &str, method: &str) -> String {
        lazy_static! {
            static ref SINGLETON_CLASS_REGEX: Regex = Regex::new("^#?<Class:([^\\s>]+)")
                .unwrap();
        }

        let class_str = Event::sanitized_class_name(class);

        if let Some(caps) = SINGLETON_CLASS_REGEX.captures(&class_str) {
            let class_name = caps.get(1).unwrap().as_str();
            format!("{}.{}", class_name, method)
        }
        else {
            format!("{}#{}", class_str, method)
        }
    }
}

#[test]
fn test_format_class() {
    let event = Event::Class("Regular::Ruby::Class".to_string());
    assert_eq!(event.format(), "Regular::Ruby::Class");

    let event = Event::Class("#<Some::SingletonClass:0xF2F5EAB2B2D35910>".to_string());
    assert_eq!(event.format(), "#<Some::SingletonClass:(generated)>");
}

#[test]
fn test_format_call() {
    let event = Event::Call("Regular::Ruby::Class".to_string(), "just_do_it".to_string());
    assert_eq!(event.format(), "Regular::Ruby::Class#just_do_it");

    let event = Event::Call("#<Class:Object>".to_string(), "compute".to_string());
    assert_eq!(event.format(), "Object.compute");
}

pub struct Frame {
    event: Event,
    caller_file: String,
    caller_line: i32,
    locals: FnvHashMap<String, FnvHashSet<String>>,
    return_type: String
}

impl Frame {
    pub fn new(event: Event, caller_file: String, caller_line: i32) -> Frame {
        Frame {
            event: event,
            caller_file: caller_file,
            caller_line: caller_line,
            locals: FnvHashMap::default(),
            return_type: String::new()
        }
    }

    /// Registers the local with the given type. If the local is already present,
    /// the given type is added to its list of types.
    pub fn add_local(&mut self, var_name: &str, type_name: &str) {
        if let Some(set) = self.locals.get_mut(var_name) {
            set.insert(type_name.to_string());
        }
        else {
            let mut set = FnvHashSet::default();
            set.insert(type_name.to_string());
            self.locals.insert(var_name.to_string(), set);
        }
    }

    pub fn set_return_type(&mut self, value: String) {
        self.return_type = value;
    }

    pub fn format(&self) -> String { self.event.format() }

    /// Write to filesystem
    pub fn write(&mut self, cwd: PathBuf) {
        // TODO this method is FULL of copy/paste
        let exposure_path = cwd.join(".exposure");
        let formatted = self.format();

        ///////////// First, locals //////////////
        for (local, types) in &mut self.locals {
            let filename = format!("{}%{}", formatted, local);
            let locals_path = exposure_path.join("locals").join(&filename);
            let original_len = types.len();

            // Merge existing data with ours
            match read_lines(locals_path.clone()) {
                Ok(lines) => {
                    for line in lines {
                        types.insert(line.expect("Failed to read line from locals file"));
                    }
                }
                _ => {} // Doesn't matter if we can't do it
            }

            // Duck out if nothing changed
            if types.len() == original_len { continue }

            // Write it all back
            let mut file = File::create(locals_path).expect("Failed to open locals file for write");
            for typename in types.iter() {
                let bytes: Vec<u8> = typename.bytes().collect();
                file.write_all(&bytes).unwrap();
                file.write_all(b"\n").unwrap();
            }
        }

        ///////////// Second, returns ///////////////
        if !self.return_type.is_empty() {
            let returns_path = exposure_path.join("returns").join(&formatted);

            let mut return_types = FnvHashSet::<String>::default();
            return_types.insert(self.return_type.clone());

            // Merge existing data with ours
            match read_lines(returns_path.clone()) {
                Ok(lines) => {
                    for line in lines {
                        return_types.insert(line.expect("Failed to read line from returns file"));
                    }
                }
                _ => {} // Doesn't matter if we can't do it
            }

            // Duck out if nothing changed
            if return_types.len() != 1 {
                // Write it all back
                let mut file = File::create(returns_path).expect("Failed to open returns file for write");
                for typename in return_types.iter() {
                    let bytes: Vec<u8> = typename.bytes().collect();
                    file.write_all(&bytes).unwrap();
                    file.write_all(b"\n").unwrap();
                }
            }
        }

        ///////////// Third, usages ///////////////
        // TODO this actually seems like it has a high risk of becoming wrong.
        //      might want to delete entries that share our filename?
        if false {
            match self.event { Event::Class(_) => return, _ => {} }
            let uses_path = exposure_path.join("uses").join(&formatted);

            let mut uses = FnvHashSet::<String>::default();
            let my_use = format!("{}:{}", self.caller_file, self.caller_line);
            uses.insert(my_use);

            // Merge existing data with ours
            match read_lines(uses_path.clone()) {
                Ok(lines) => {
                    for line in lines {
                        uses.insert(line.expect("Failed to read line from returns file"));
                    }
                }
                _ => {} // Doesn't matter if we can't do it
            }

            // Write it all back
            let mut file = File::create(uses_path).expect("Failed to open returns file for write");
            for usage in uses.iter() {
                let bytes: Vec<u8> = usage.bytes().collect();
                file.write_all(&bytes).unwrap();
                file.write_all(b"\n").unwrap();
            }
        }
    }
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where P: AsRef<Path> {
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}


/// A single call stack trace.
pub struct Trace {
    frames: Vec<Frame>,
    cwd: PathBuf,
    writer: mpsc::Sender<Option<Frame>>,
    writer_thread: JoinHandle<()>
}

impl Trace {
    pub fn new() -> Trace {
        let (tx, rx): (mpsc::Sender<Option<Frame>>, _) = mpsc::channel();

        let cwd = env::current_dir().expect("Could not read CWD");
        let thread_cwd = cwd.clone();

        let join_handle = thread::spawn(move || {
            loop {
                match rx.recv() {
                    Ok(Some(mut frame)) => frame.write(thread_cwd.clone()),
                    _ => break
                }
            };
        });

        Trace {
            frames: vec![],
            cwd: cwd,
            writer: tx,
            writer_thread: join_handle
        }
    }

    pub fn push(&mut self, frame: Frame) {
        self.frames.push(frame);
    }

    pub fn pop(&mut self) -> Option<Frame> {
        self.frames.pop()
    }

    pub fn pop_and_write(&mut self, return_type: String) {
        match self.frames.pop() {
            Some(mut frame) => {
                frame.set_return_type(return_type);
                self.writer.send(Some(frame)).expect("Writer thread died somehow");
            }
            _ => return
        };
    }

    pub fn top(&mut self) -> Option<&mut Frame> {
        self.frames.last_mut()
    }

    pub fn current_dir(&self) -> PathBuf {
        self.cwd.clone()
    }

    pub fn finish(self) {
        self.writer.send(None).expect("Couldn't tell writer thread to stop");
        self.writer_thread.join().expect("Writer panicked at some point");
    }
}


impl fmt::Debug for EventType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            EventType::BCall => write!(f, ":b_call"),
            EventType::Class => write!(f, ":class"),
            EventType::Call => write!(f, ":call"),
            EventType::Return => write!(f, ":return"),
            EventType::BReturn => write!(f, ":b_return"),
            EventType::End => write!(f, ":end"),
        }
    }
}

impl EventType {
    pub fn from_int(i: i32) -> EventType {
        match i {
            1 => EventType::BCall,
            2 => EventType::Class,
            3 => EventType::Call,
            4 => EventType::Return,
            5 => EventType::BReturn,
            6 => EventType::End,
            _ => panic!("this cant be happening")
        }
    }
}



// C-style alloc function. Caller (ruby) should manage this memory!
#[no_mangle]
pub extern "C" fn create_trace() -> *const c_void {
    let trace = Box::new(Trace::new());

    // Create necessary directories
    let exposure_path = trace.current_dir().join(".exposure");
    fs::create_dir_all(exposure_path.join("locals")).expect("Couldn't create locals dir");
    fs::create_dir_all(exposure_path.join("returns")).expect("Couldn't create returns dir");
    fs::create_dir_all(exposure_path.join("uses")).expect("Couldn't create uses dir");

    Box::into_raw(trace) as *const c_void
}

// C-style dealloc function.
#[no_mangle]
pub extern "C" fn destroy_trace(trace_ptr: *mut c_void) {
    unsafe {
        let trace = Box::from_raw(trace_ptr as *mut Trace);
        trace.finish();
    }
}

// Push a new frame and return a pointer to it. That pointer should not
// be accessed after pushing again!
#[no_mangle]
pub extern "C" fn push_frame(
    trace_ptr: *mut c_void,

    event_type_int: i32,

    caller_file_cstr: *mut c_char,
    caller_line:      i32,

    trace_file_cstr: *mut c_char,
    trace_line:      i32,

    class_name_cstr: *mut c_char,
    method_id_cstr: *mut c_char,

    receiver_cstr:   *mut c_char
) {
    let trace = unsafe { Box::leak(Box::from_raw(trace_ptr as *mut Trace)) };
    let event_type = EventType::from_int(event_type_int);

    let caller_file = cstr_to_string(caller_file_cstr);
    let trace_file = cstr_to_string(trace_file_cstr);
    let class_name = cstr_to_string(class_name_cstr);
    let method_id = cstr_to_string(method_id_cstr);
    let receiver = cstr_to_string(receiver_cstr);

    let event = match event_type {
        EventType::BCall => Event::BCall(trace_file, trace_line),
        EventType::Class => Event::Class(receiver),
        EventType::Call  => Event::Call(class_name, method_id),
        wrong_type => {
            panic!("YOU IDIOT!! DON'T PUSH FOR {:?}", wrong_type);
        }
    };

    let frame = Frame::new(event, caller_file, caller_line);
    trace.push(frame);
}

// Add a local to the top frame.
#[no_mangle]
pub extern "C" fn add_local(
    trace_ptr: *mut c_void,
    name_cstr: *mut c_char,
    type_cstr: *mut c_char
) {
    let trace = unsafe { Box::leak(Box::from_raw(trace_ptr as *mut Trace)) };
    let frame = match trace.top() { Some(f) => f, None => return };

    let local_var_name = cstr_to_string(name_cstr);
    let local_var_class = cstr_to_string(type_cstr);
    let local_var_type = Event::sanitized_class_name(&local_var_class).to_string();

    frame.add_local(&local_var_name, &local_var_type);
}

// Just pop the last frame off the stack. Register its return type while you're at it.
#[no_mangle]
pub extern "C" fn pop_frame(
    trace_ptr: *mut c_void,
    return_type_cstr: *mut c_char,
) {
    let trace = unsafe { Box::leak(Box::from_raw(trace_ptr as *mut Trace)) };
    let return_class_name = cstr_to_string(return_type_cstr);

    let return_type = Event::sanitized_class_name(&return_class_name);
    trace.pop_and_write(return_type.into_owned());
}


fn cstr_to_string(cstr: *const c_char) -> String {
    if cstr.is_null() { return String::new() }
    unsafe { CStr::from_ptr(cstr).to_string_lossy().into_owned() }
}
