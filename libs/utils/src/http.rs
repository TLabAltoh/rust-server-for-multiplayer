use std::collections::HashMap;

pub fn get_from_params_string(params: &HashMap<String, String>, name: &str, value: &mut String) -> bool {
    if let Some(n) = params.get(name) {
        value.clear();
        value.push_str(n);
    } else {
        return false;
    };

    return true;
}

pub fn get_from_params_u32(params: &HashMap<String, String>, name: &str, value: &mut u32) -> bool {
    if let Some(n) = params.get(name) {
        let p: std::prelude::v1::Result<u32, std::num::ParseIntError> = n.parse::<u32>();
        if p.is_ok() {
            *value = p.unwrap();
        } else {
            return false;
        }
    } else {
        return false;
    };

    return true;
}

pub fn get_from_params_u64(params: &HashMap<String, String>, name: &str, value: &mut u64) -> bool {
    if let Some(n) = params.get(name) {
        let p: std::prelude::v1::Result<u64, std::num::ParseIntError> = n.parse::<u64>();
        if p.is_ok() {
            *value = p.unwrap();
        } else {
            return false;
        }
    } else {
        return false;
    };

    return true;
}

pub fn get_from_params_bool(params: &HashMap<String, String>, name: &str, value: &mut bool) -> bool {
    if let Some(n) = params.get(name) {
        match n.as_str() {
            "on" => *value = true,
            _ => *value=false   // if check box is off, parameter not contain this param.
        };
    } else {
        *value=false;
    };

    return true;
}