#[macro_use]
extern crate lazy_static;
extern crate regex;

use regex::Regex;
use fnv::{FnvHashMap, FnvHashSet};

// use std::thread;
use std::fmt;
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
//use std::sync::mpsc;
use std::alloc::{alloc, dealloc, Layout};
use std::borrow::Cow;

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
            // TODO this is not good
            Event::BCall(file, line)   => format!("{}#{}", file, line)
        }
    }

    fn sanitized_class_name(class_name: &str) -> Cow<str> {
        lazy_static! {
            static ref GENERATED_ID_REGEX: Regex = Regex::new(":0x[\\dA-Fa-f]{16}")
                .unwrap();
        }
        GENERATED_ID_REGEX.replace_all(class_name, "(generated)")
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
    assert_eq!(event.format(), "#<Some::SingletonClass(generated)>");
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
    locals: FnvHashMap<String, FnvHashSet<String>>
}

impl Frame {
    pub fn new(event: Event, caller_file: String, caller_line: i32) -> Frame {
        Frame {
            event: event,
            caller_file: caller_file,
            caller_line: caller_line,
            locals: FnvHashMap::default(),
        }
    }

    /// Registers the local with the given type. If the local is already present,
    /// the given type is added to its list of types.
    pub fn add_local(&mut self, var_name: String, type_name: String) {
        if let Some(set) = self.locals.get_mut(&var_name) {
            set.insert(type_name);
        }
        else {
            let mut set = FnvHashSet::default();
            set.insert(type_name);
            self.locals.insert(var_name, set);
        }
    }
}


/// A single call stack trace.
pub struct Trace {
    frames: Vec<Frame>
}

impl Trace {
    pub fn new() -> Trace {
        Trace {
            frames: vec![]
        }
    }

    pub fn push(&mut self, frame: Frame) {
        self.frames.push(frame);
    }

    pub fn pop(&mut self) {
        self.frames.pop();
    }

    pub fn top(&mut self) -> Option<&mut Frame> {
        self.frames.last_mut()
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
    unsafe {
        let layout = Layout::new::<Trace>();
        let ptr = alloc(layout);
        *(ptr as *mut Trace) = Trace::new();
        ptr as *const c_void
    }
}

// C-style dealloc function.
#[no_mangle]
pub extern "C" fn destroy_trace(trace_ptr: *mut c_void) {
    unsafe {
        let layout = Layout::new::<Trace>();
        dealloc(trace_ptr as *mut u8, layout);
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
    let trace: &mut Trace = unsafe { &mut (*(trace_ptr as *mut Trace)) };
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
    let trace: &mut Trace = unsafe { &mut (*(trace_ptr as *mut Trace)) };
    let frame = match trace.top() { Some(f) => f, None => return };

    let local_var_name = cstr_to_string(name_cstr);
    let local_var_class = cstr_to_string(type_cstr);
    let local_var_type = Event::sanitized_class_name(&local_var_class).to_string();

    frame.add_local(local_var_name, local_var_type);
}

// Just pop the last frame off the stack. Register its return type while you're at it.
#[no_mangle]
pub extern "C" fn pop_frame(
    trace_ptr: *mut c_void,
    return_type_cstr: *mut c_char,
) {
    let trace: &mut Trace = unsafe { &mut (*(trace_ptr as *mut Trace)) };
    let return_class_name = cstr_to_string(return_type_cstr);
    let return_type = Event::sanitized_class_name(&return_class_name);
    // TODO actually do something with return type
    println!("Popping {}", return_type);

    trace.pop();
}


fn cstr_to_string(cstr: *const c_char) -> String {
    if cstr.is_null() { return String::new() }
    unsafe { CStr::from_ptr(cstr).to_string_lossy().into_owned() }
}
