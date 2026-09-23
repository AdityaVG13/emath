use super::{JsonValue, Value, field, text, BTreeMap, MethodFrame, JsonWriter, SavedRun, Path, CHECKPOINT_SCHEMA, content_id_of_str, STAGING_SEQUENCE, Ordering, Write};

pub(super) fn unsigned(doc: &JsonValue, name: &str) -> Result<usize, String> {
    usize::try_from(doc.int_field(name).map_err(|error| error.to_string())?).map_err(|error| error.to_string())
}

pub(super) fn saved_value(doc: &JsonValue) -> Result<Value, String> {
    let invalid = || "E-RUN-VALUE: invalid exact checkpoint value".to_string();
    let boolean = |name| match field(doc, name)? { JsonValue::Bool(value) => Ok(*value), _ => Err(invalid()) };
    let float = |bits: &str| {
        if bits.len() != 16 { return Err(invalid()); }
        u64::from_str_radix(bits, 16).map(f64::from_bits).map_err(|_| invalid())
    };
    let floats = |name| doc.strings_field(name).map_err(|error| error.to_string())?.iter().map(|bits| float(bits)).collect::<Result<Vec<_>, _>>();
    let elements = || match field(doc, "elements")? {
        JsonValue::Arr(values) => values.iter().map(saved_value).collect::<Result<Vec<_>, _>>(), _ => Err(invalid()),
    };
    let value = match text(doc, "type")?.as_str() {
        "Int" => Value::I64(text(doc, "value")?.parse().map_err(|_| invalid())?),
        "Float64" => Value::F64(float(&text(doc, "bits")?)?),
        "Bool" => Value::Bool(text(doc, "value")?.parse().map_err(|_| invalid())?),
        "Text" => Value::Text(text(doc, "value")?),
        "BigInt" => Value::parse_bigint(&text(doc, "value")?).ok_or_else(invalid)?,
        "Rat" => {
            let num = text(doc, "numerator")?.parse::<i128>().map_err(|_| invalid())?;
            let den = text(doc, "denominator")?.parse::<i128>().map_err(|_| invalid())?;
            let ratio = emath_rt::ratio_norm((num, den))?;
            if ratio != (num, den) { return Err(invalid()); }
            Value::Rat { num, den }
        }
        "Record" => {
            let JsonValue::Obj(entries) = field(doc, "fields")? else { return Err(invalid()); };
            let mut fields = BTreeMap::new();
            for (name, value) in entries {
                if fields.insert(name.clone(), saved_value(value)?).is_some() { return Err(invalid()); }
            }
            Value::Record { type_name: text(doc, "name")?, fields }
        }
        "List" => Value::List(elements()?),
        "Set" => Value::Set(elements()?),
        "Vector<Float64>" => Value::Vector(floats("bits")?),
        "Vector<BigInt>" => Value::BigVector(doc.strings_field("elements").map_err(|error| error.to_string())?.iter().map(|value| {
            match Value::parse_bigint(value) { Some(Value::BigInt(value)) => Ok(value), _ => Err(invalid()) }
        }).collect::<Result<Vec<_>, _>>()?),
        "Matrix<Float64>" => {
            let rows = unsigned(doc, "rows")?;
            let cols = unsigned(doc, "columns")?;
            let data = floats("bits")?;
            if rows.checked_mul(cols) != Some(data.len()) { return Err(invalid()); }
            Value::Matrix { rows, cols, data }
        }
        "Tensor<Float64>" => {
            let shape = doc.strings_field("shape").map_err(|error| error.to_string())?.iter().map(|size| size.parse::<usize>().map_err(|_| invalid())).collect::<Result<Vec<_>, _>>()?;
            let data = floats("bits")?;
            if shape.iter().try_fold(1_usize, |size, dimension| size.checked_mul(*dimension)) != Some(data.len()) { return Err(invalid()); }
            Value::Tensor { shape, data }
        }
        "Complex" | "Interval<Float64>" => {
            let data = floats("bits")?;
            let [left, right] = data.as_slice() else { return Err(invalid()); };
            if text(doc, "type")? == "Complex" { Value::Complex { re: *left, im: *right } }
            else { Value::Interval { lo: *left, hi: *right } }
        }
        "Option" => Value::Option(if boolean("some")? { Some(Box::new(saved_value(field(doc, "payload")?)?)) } else { None }),
        "Result" => Value::Result { ok: boolean("ok")?, payload: Box::new(saved_value(field(doc, "payload")?)?) },
        "Series" => {
            let times = floats("time_bits")?;
            let values = floats("value_bits")?;
            if times.len() != values.len() { return Err(invalid()); }
            Value::Series { points: times.into_iter().zip(values).collect(), interpolation: text(doc, "interpolation")?, extrapolation: text(doc, "extrapolation")? }
        }
        _ => return Err("E-RUN-VALUE: this carrier cannot be saved as a method argument".into()),
    };
    if value.to_string() != text(doc, "value")? { return Err(invalid()); }
    Ok(value)
}

pub(super) fn frames_json(frames: &[MethodFrame]) -> Vec<String> {
    frames.iter().map(|frame| {
        let mut out = JsonWriter::object();
        out.string("key", &frame.key);
        out.string("capability", &frame.capability);
        out.objects("arguments", &frame.arguments.iter().map(Value::json).collect::<Vec<_>>());
        out.object_field("state", &frame.state.json());
        if let Some(fault) = &frame.fault { out.string("fault", fault); } else { out.field("fault", "null"); }
        out.finish()
    }).collect()
}

pub(super) fn saved_frames(doc: &JsonValue) -> Result<Vec<MethodFrame>, String> {
    let JsonValue::Arr(entries) = doc else { return Err("invalid method frames".into()); };
    let mut frames: Vec<MethodFrame> = Vec::new();
    for entry in entries {
        let key = text(entry, "key")?;
        if frames.iter().any(|frame| frame.key == key) { return Err("duplicate method frame".into()); }
        let JsonValue::Arr(arguments) = field(entry, "arguments")? else { return Err("invalid method arguments".into()); };
        let fault = match field(entry, "fault")? { JsonValue::Str(fault) => Some(fault.clone()), JsonValue::Null => None, _ => return Err("invalid method fault".into()) };
        frames.push(MethodFrame { key, capability: text(entry, "capability")?, arguments: arguments.iter().map(saved_value).collect::<Result<_, _>>()?, state: saved_value(field(entry, "state")?)?, fault });
    }
    Ok(frames)
}

pub(super) fn save(state: &SavedRun, destination: &Path) -> Result<(), String> {
    let payload = state.payload();
    let mut envelope = JsonWriter::object();
    envelope.string("schema", CHECKPOINT_SCHEMA);
    envelope.string("content_id", &content_id_of_str(&payload).0);
    envelope.string("payload", &payload);
    save_document(&envelope.finish(), destination)
}

pub(super) fn save_document(bytes: &str, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or("checkpoint has no parent directory")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let staging = parent.join(format!(
        ".run-{}-{}.tmp",
        std::process::id(),
        STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)
        .map_err(|error| error.to_string())?;
    let written = file
        .write_all(bytes.as_bytes())
        .and_then(|()| file.sync_all());
    drop(file);
    let result = written.and_then(|()| std::fs::hard_link(&staging, destination));
    // Only this invocation's staging file is removed. Published checkpoints
    // are immutable; concurrent requests cannot replace one another's data.
    let _ = std::fs::remove_file(&staging);
    match result {
        Ok(()) => std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if std::fs::read_to_string(destination).ok().as_deref() == Some(bytes) {
                Ok(())
            } else {
                Err("checkpoint publication conflicts with an existing result".into())
            }
        }
        Err(error) => Err(error.to_string()),
    }
}

