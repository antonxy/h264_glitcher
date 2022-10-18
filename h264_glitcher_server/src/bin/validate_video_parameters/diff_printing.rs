use colored::Colorize;
use visit_diff::record::*;
use visit_diff::Diff;

fn remove_same_struct(value: Struct) -> Option<Struct> {
    let fields: Vec<_> = value
        .fields
        .into_iter()
        .map(|(k, v)| (k, v.and_then(remove_same)))
        .filter(|(_k, v)| v.is_some())
        .collect();

    if fields.len() > 0 {
        Some(Struct {
            name: value.name,
            fields,
        })
    } else {
        None
    }
}

fn remove_same_enum(value: Enum) -> Option<Enum> {
    let new_variant = match value.variant {
        Variant::Struct(s) => remove_same_struct(s).map(|v| Variant::Struct(v)),
        Variant::Tuple(t) => remove_same_tuple(t).map(|v| Variant::Tuple(v)),
    };
    if let Some(new_variant) = new_variant {
        Some(Enum {
            name: value.name,
            variant: new_variant,
        })
    } else {
        None
    }
}

fn remove_same_tuple(value: Tuple) -> Option<Tuple> {
    let fields: Vec<_> = value
        .fields
        .into_iter()
        .map(|v| v.and_then(remove_same))
        .filter(|v| v.is_some())
        .collect();

    if fields.len() > 0 {
        Some(Tuple {
            name: value.name,
            fields,
        })
    } else {
        None
    }
}

fn remove_same(value: Value) -> Option<Value> {
    match value {
        Value::Same(_, _) => None,
        Value::Difference(_, _) => Some(value),
        Value::Struct(s) => remove_same_struct(s).map(|v| Value::Struct(v)),
        Value::Tuple(t) => remove_same_tuple(t).map(|v| Value::Tuple(v)),
        Value::Enum(e) => remove_same_enum(e).map(|v| Value::Enum(v)),
        _ => unimplemented!(),
    }
}

pub fn print_diff<D: Diff>(reference: &D, value: &D) {
    let record = record_diff(reference, value);
    let differences = remove_same(record);
    if let Some(differences) = differences {
        print_value(differences, None, 0);
    }
}

fn print_value(value: Value, name: Option<&str>, indent: usize) {
    match value {
        Value::Same(_, _) => {}
        Value::Difference(a, b) => print_difference(a, b, name.unwrap(), indent),
        Value::Struct(s) => print_struct(s, indent),
        Value::Tuple(t) => print_tuple(t, indent),
        Value::Enum(e) => print_enum(e, name.unwrap(), indent),
        _ => unimplemented!(),
    }
}

fn print_difference(a: String, b: String, name: &str, indent: usize) {
    println!(
        "{:indent$}{}: {}, {}",
        "",
        name,
        a.green(),
        b.red(),
        indent = indent
    )
}

fn print_struct(value: Struct, indent: usize) {
    println!("{:indent$}{} {{", "", value.name, indent = indent);
    for (k, v) in value.fields {
        print_value(v.unwrap(), Some(k), indent + 1);
    }
    println!("{:indent$}}}", "", indent = indent);
}

fn print_enum(value: Enum, name: &str, indent: usize) {
    println!("{:indent$}{}:", "", name, indent = indent);
    match value.variant {
        Variant::Struct(s) => {
            print_struct(s, indent + 1);
        }
        Variant::Tuple(t) => {
            // Skip "Some(" around optional value
            if value.name == "Option" {
                for v in t.fields {
                    print_value(v.unwrap(), None, indent + 1);
                }
            } else {
                print_tuple(t, indent + 1);
            }
        }
    };
}

fn print_tuple(value: Tuple, indent: usize) {
    println!("{:indent$}{}(", "", value.name, indent = indent);
    for v in value.fields {
        print_value(v.unwrap(), None, indent + 1);
    }
    println!("{:indent$})", "", indent = indent);
}
