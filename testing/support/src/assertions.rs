use super::*;

pub fn contains(haystack: &str, needle: &str) -> Result<()> {
    if haystack.contains(needle) {
        Ok(())
    } else {
        bail!("expected output to contain `{needle}`\noutput:\n{haystack}");
    }
}

pub fn not_contains(haystack: &str, needle: &str) -> Result<()> {
    if haystack.contains(needle) {
        bail!("expected output NOT to contain `{needle}`\noutput:\n{haystack}");
    }
    Ok(())
}

pub fn assert_schema_title(path: &Path, expected_title: &str) -> Result<Value> {
    let value: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    if value.get("$schema").is_none() {
        bail!("schema {} has no `$schema` field", path.display());
    }
    if value.get("title").and_then(Value::as_str) != Some(expected_title) {
        bail!(
            "schema {} title was not `{expected_title}`: {value}",
            path.display()
        );
    }
    Ok(value)
}

pub fn assert_json_log(path: &Path, expected: &[(&str, &str)]) -> Result<()> {
    let contents = fs::read_to_string(path)?;
    for line in contents.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if expected
            .iter()
            .all(|(key, expected)| value.get(*key).and_then(Value::as_str) == Some(*expected))
        {
            return Ok(());
        }
    }
    bail!(
        "log {} did not contain JSON line with fields {:?}\ncontents:\n{}",
        path.display(),
        expected,
        contents
    );
}

pub fn assert_structured_field(output: &str, field: &str, expected: &str) -> Result<()> {
    let structured = output
        .lines()
        .find_map(|line| line.strip_prefix("structured: "))
        .ok_or_else(|| anyhow!("command output had no structured content:\n{output}"))?;
    let structured: Value = serde_json::from_str(structured)?;
    if structured.get(field).and_then(Value::as_str) == Some(expected) {
        Ok(())
    } else {
        bail!("structured field `{field}` did not equal `{expected}`: {structured}");
    }
}

pub fn task_id_from_output(output: &str) -> Result<String> {
    output
        .lines()
        .find_map(|line| {
            line.strip_prefix("task ")
                .and_then(|rest| rest.split_whitespace().next())
                .map(str::to_string)
        })
        .ok_or_else(|| anyhow!("command output had no task id:\n{output}"))
}

pub fn structured_from_output<T: DeserializeOwned>(output: &str) -> Result<T> {
    let structured = output
        .lines()
        .find_map(|line| line.strip_prefix("structured: "))
        .ok_or_else(|| anyhow!("command output had no structured content:\n{output}"))?;
    Ok(serde_json::from_str(structured)?)
}

pub fn assert_output_file(output_dir: &Path, extension: &str) -> Result<()> {
    if contains_nonempty_file_with_extension(output_dir, extension)? {
        Ok(())
    } else {
        bail!(
            "no non-empty .{extension} output file found under {}",
            output_dir.display()
        );
    }
}

pub fn contains_nonempty_file_with_extension(path: &Path, extension: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if contains_nonempty_file_with_extension(&path, extension)? {
                return Ok(true);
            }
        } else if path.extension().and_then(|ext| ext.to_str()) == Some(extension)
            && entry.metadata()?.len() > 0
        {
            return Ok(true);
        }
    }
    Ok(false)
}
