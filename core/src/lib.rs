// use std::thread;
use std::fmt;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::mpsc;

/// Enum corresponding to the tracepoint events we are about
pub enum Event {
    BCall = 1,
    Class = 2,
    Call = 3,
    Return = 4,
    BReturn = 5,
    End = 6,
}

/// Represents one stack frame
pub struct Frame {
    event: Event,
    file: String,
    line: i32,
    method_id: String,
    local_names: Vec<String>,
    local_types: Vec<String>,
    class_name: String
}

/// State we use in the processing thread
pub struct Backend {
    receiver: mpsc::Receiver<Frame>,
    frames: Vec<Frame>
}

/// State managed by ruby caller
pub struct Frontend {
    sender: mpsc::Sender<Frame>,
}


impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Event::BCall => write!(f, ":b_call"),
            Event::Class => write!(f, ":class"),
            Event::Call => write!(f, ":call"),
            Event::Return => write!(f, ":return"),
            Event::BReturn => write!(f, ":b_return"),
            Event::End => write!(f, ":end"),
        }
    }
}

impl Event {
    pub fn from_int(i: i32) -> Event {
        match i {
            1 => Event::BCall,
            2 => Event::Class,
            3 => Event::Call,
            4 => Event::Return,
            5 => Event::BReturn,
            6 => Event::End,
            _ => panic!("this cant be happening")
        }
    }
}




#[no_mangle]
pub extern "C" fn go_and_test(
    event_int: i32,
    file_cstr: *mut c_char,
    line: i32,
    method_id_cstr: *mut c_char
    ) {
    let event = Event::from_int(event_int);
    let file = cstr_to_string(file_cstr);
    let method_id = cstr_to_string(method_id_cstr);

    println!("You've done it! {:?} {:?} {} {:?}", event, file, line, method_id);
}


fn cstr_to_string(cstr: *const c_char) -> String {
    unsafe { CStr::from_ptr(cstr).to_string_lossy().into_owned() }
}


#[test]
fn it_works() {
    let e = Event::Call;
    let f = Frame {
        event: e,
        file: "".to_string(),
        line: 0,
        method_id: "".to_string(),
        local_names: vec![],
        local_types: vec![],
        class_name: "".to_string(),
    };

    assert_eq!(2 + f.event as i32, 5);
}
