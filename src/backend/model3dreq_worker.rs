use super::*;

const TOO_LARGE_EXIT: i32 = 3;

pub(super) fn failure(status: std::process::ExitStatus, reason: String) -> Failure {
    Failure {
        reason,
        refusal: (status.code() == Some(TOO_LARGE_EXIT)).then_some(Refusal::TooLarge),
    }
}

fn result_code(result: &Result<(), Failure>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(failure) if failure.refusal == Some(Refusal::TooLarge) => TOO_LARGE_EXIT,
        Err(_) => 1,
    }
}

// The model path arrives as raw bytes, which args() would panic on, so preserve its exact text.
pub(super) fn args(raw: impl IntoIterator<Item = std::ffi::OsString>) -> Vec<String> {
    raw.into_iter()
        .map(|arg| arg.exact_text().into_owned())
        .collect()
}

// run.rs selects this before loading backend tables; the helper has one file and no nested tools.
pub(super) fn entry() -> Option<i32> {
    let args = args(std::env::args_os());
    if !args.iter().any(|s| s == "--3d-worker") {
        return None;
    }
    let result = (|| -> Result<(), Failure> {
        if args.len() != 6 || args[1] != "--backend" || args[2] != "--3d-worker" || args[4] != "--"
        {
            return Err("invalid 3D helper arguments".into());
        }
        let file = regfile::open_if_regular(&flea::bytename::view(&args[5]), 0)
            .map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.take(model3d::MAX_INPUT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        // The XML budget charges the input by its length, so read_to_end's spare capacity is released first.
        let bytes = bytes.into_boxed_slice();
        let model = model3d::parse_classified(&args[3], &bytes)?;
        model3d::wire::write(&model, std::io::BufWriter::new(std::io::stdout())).map_err(Into::into)
    })();
    if let Err(failure) = &result {
        eprintln!("flea: {}", failure.reason);
    }
    Some(result_code(&result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn worker_result_codes_round_trip_the_typed_refusal() {
        assert_eq!(result_code(&Ok(())), 0);
        for refusal in [None, Some(Refusal::TooLarge)] {
            let expected = if refusal.is_some() { 3 } else { 1 };
            let result = Err(Failure {
                reason: "diagnostic may change".into(),
                refusal,
            });
            let code = result_code(&result);
            assert_eq!(code, expected);
            let decoded = failure(
                std::process::ExitStatus::from_raw(code << 8),
                "other wording".into(),
            );
            assert_eq!(decoded.refusal, refusal);
        }
    }
}
