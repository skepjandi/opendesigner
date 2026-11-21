//! Speech to text with fake programs (`tests/fakes.rs`): detection, arguments, parsing, limits.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod fakes;

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use fakes::*;
use od_media::external::{ExternalError, Limits, Scratch, Search, Whisper, WhisperFlavor, run};
use od_media::speech::{self, Model, Request};

fn search_in(dir: &std::path::Path) -> Search {
    Search {
        explicit: None,
        app_dirs: vec![],
        path_dirs: vec![dir.to_path_buf()],
    }
}

#[test]
fn whisper_cpp_wins_over_the_python_whisper_and_the_app_folder_over_path() {
    let tmp = tempfile::tempdir().unwrap();
    let (app, path) = (tmp.path().join("app"), tmp.path().join("path"));
    std::fs::create_dir_all(&app).unwrap();
    std::fs::create_dir_all(&path).unwrap();
    fake_openai(&path, "whisper");
    let only_python = Search {
        explicit: None,
        app_dirs: vec![app.clone()],
        path_dirs: vec![path.clone()],
    };
    let found = Whisper::locate_in(&only_python).unwrap();
    assert_eq!(found.flavor, WhisperFlavor::OpenAi);

    fake_cpp(&path, "whisper-cli");
    assert_eq!(
        Whisper::locate_in(&only_python).unwrap().path,
        path.join("whisper-cli")
    );

    fake_cpp(&app, "whisper-cli");
    assert_eq!(
        Whisper::locate_in(&only_python).unwrap().path,
        app.join("whisper-cli")
    );
}

#[test]
fn a_file_that_is_not_executable_is_not_a_program_and_nothing_found_says_what_to_install() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("whisper-cli"), "not a program").unwrap();
    let err = Whisper::locate_in(&search_in(tmp.path())).unwrap_err();
    let text = err.to_string();
    assert!(
        text.contains("whisper-cpp") && text.contains("openai-whisper"),
        "{text}"
    );
    assert!(text.contains("OPENDESIGNER_WHISPER"), "{text}");
}

#[test]
fn an_override_wins_but_a_bad_one_is_an_error_not_a_reason_to_look_elsewhere() {
    let tmp = tempfile::tempdir().unwrap();
    fake_cpp(tmp.path(), "whisper-cli");
    let custom = fake_openai(tmp.path(), "custom-stt");
    let mut search = search_in(tmp.path());
    search.explicit = Some(custom.clone().into_os_string());
    let found = Whisper::locate_in(&search).unwrap();
    assert_eq!(
        (found.path.clone(), found.flavor),
        (custom, WhisperFlavor::Cpp)
    );

    search.explicit = Some(tmp.path().join("missing").into_os_string());
    assert!(matches!(
        Whisper::locate_in(&search),
        Err(ExternalError::BadOverride { .. })
    ));
}

fn request<'a>(
    wav: &'a std::path::Path,
    model: &'a Model,
    scratch: &'a std::path::Path,
) -> Request<'a> {
    Request {
        wav,
        model,
        language: "en",
        scratch,
        timeout: Duration::from_secs(20),
        cancel: None,
    }
}

#[test]
fn whisper_cpp_gets_validated_arguments_and_its_json_becomes_words() {
    let tmp = tempfile::tempdir().unwrap();
    let program = Whisper {
        path: fake_cpp(tmp.path(), "whisper-cli"),
        flavor: WhisperFlavor::Cpp,
    };
    let model_file = tmp.path().join("ggml-tiny.bin");
    std::fs::write(&model_file, b"model").unwrap();
    let model = Model::for_flavor(WhisperFlavor::Cpp, model_file.to_str()).unwrap();
    let scratch = Scratch::new().unwrap();
    let wav = scratch.path().join("in.wav");
    std::fs::write(&wav, b"RIFF").unwrap();

    let words = speech::transcribe(&program, &request(&wav, &model, scratch.path())).unwrap();

    let texts: Vec<_> = words.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(
        texts,
        ["Hello", "world.", "Again"],
        "marker dropped, dot glued"
    );
    assert_eq!((words[0].start_ms, words[1].end_ms), (200, 950));
    let args = recorded(tmp.path(), "args.txt");
    let expected: Vec<String> = [
        "-m",
        model_file.to_str().unwrap(),
        "-f",
        wav.to_str().unwrap(),
        "-l",
        "en",
        "-ml",
        "1",
        "-sow",
        "-np",
        "-oj",
        "-of",
        scratch.path().join("out").to_str().unwrap(),
    ]
    .map(String::from)
    .into();
    assert_eq!(args, expected);
}

#[test]
fn openai_whisper_gets_word_timestamps_and_json_and_its_result_is_read() {
    let tmp = tempfile::tempdir().unwrap();
    let program = Whisper {
        path: fake_openai(tmp.path(), "whisper"),
        flavor: WhisperFlavor::OpenAi,
    };
    let model = Model::for_flavor(WhisperFlavor::OpenAi, Some("small.en")).unwrap();
    let scratch = Scratch::new().unwrap();
    let wav = scratch.path().join("speech.wav");
    std::fs::write(&wav, b"RIFF").unwrap();
    let mut req = request(&wav, &model, scratch.path());
    req.language = "auto";

    let words = speech::transcribe(&program, &req).unwrap();

    assert_eq!(words.len(), 3);
    assert_eq!((words[1].text.as_str(), words[1].end_ms), ("world.", 950));
    let args = recorded(tmp.path(), "args.txt");
    assert_eq!(args[0], wav.to_str().unwrap());
    assert!(args.windows(2).any(|w| w == ["--model", "small.en"]));
    assert!(args.windows(2).any(|w| w == ["--word_timestamps", "True"]));
    assert!(args.windows(2).any(|w| w == ["--output_format", "json"]));
    assert!(
        !args.iter().any(|a| a == "--language"),
        "auto leaves the language out"
    );
}

#[test]
fn the_language_and_the_model_are_validated_before_anything_runs() {
    for ok in ["auto", "en", "de", "haw"] {
        assert!(speech::language_arg(ok).is_ok(), "{ok}");
    }
    for bad in ["", "EN", "english", "e", "en;rm", "-x", "../"] {
        assert!(speech::language_arg(bad).is_err(), "{bad}");
    }
    assert!(Model::for_flavor(WhisperFlavor::OpenAi, Some("../base")).is_err());
    assert!(Model::for_flavor(WhisperFlavor::OpenAi, Some("--help")).is_err());
    assert!(Model::for_flavor(WhisperFlavor::OpenAi, Some("/models/ggml.bin")).is_err());
    assert_eq!(
        Model::for_flavor(WhisperFlavor::OpenAi, None).unwrap(),
        Model::Name("base".into())
    );
}

#[test]
fn whisper_cpp_without_a_model_or_with_a_missing_one_says_what_to_do() {
    let none = Model::for_flavor(WhisperFlavor::Cpp, None)
        .unwrap_err()
        .to_string();
    assert!(none.contains("OPENDESIGNER_WHISPER_MODEL"), "{none}");
    let gone = Model::for_flavor(WhisperFlavor::Cpp, Some("/no/such/model.bin"))
        .unwrap_err()
        .to_string();
    assert!(gone.contains("not a file"), "{gone}");
}

#[test]
fn an_agent_named_model_file_is_a_plain_name_in_the_allowed_folders_only() {
    let tmp = tempfile::tempdir().unwrap();
    let (models, media) = (tmp.path().join("models"), tmp.path().join("media"));
    std::fs::create_dir_all(&models).unwrap();
    std::fs::create_dir_all(&media).unwrap();
    std::fs::write(models.join("ggml-base.bin"), b"m").unwrap();
    std::fs::write(media.join("mine.bin"), b"m").unwrap();
    std::fs::write(tmp.path().join("outside.bin"), b"m").unwrap();
    std::os::unix::fs::symlink(tmp.path().join("outside.bin"), media.join("link.bin")).unwrap();
    let folders: Vec<PathBuf> = vec![models.clone(), media.clone()];

    assert_eq!(
        speech::model_file_in("ggml-base.bin", &folders).unwrap(),
        models.join("ggml-base.bin")
    );
    assert_eq!(
        speech::model_file_in("mine.bin", &folders).unwrap(),
        media.join("mine.bin")
    );
    for bad in [
        "../outside.bin",
        "/etc/passwd",
        ".hidden",
        "sub/x.bin",
        "",
        "link.bin",
        "absent.bin",
    ] {
        assert!(speech::model_file_in(bad, &folders).is_err(), "{bad}");
    }
}

#[test]
fn a_program_that_runs_too_long_is_stopped() {
    let tmp = tempfile::tempdir().unwrap();
    let program = Whisper {
        path: script(tmp.path(), "whisper-cli", "sleep 30"),
        flavor: WhisperFlavor::Cpp,
    };
    let model = Model::File(tmp.path().join("m.bin"));
    let scratch = Scratch::new().unwrap();
    let wav = scratch.path().join("in.wav");
    let mut req = request(&wav, &model, scratch.path());
    req.timeout = Duration::from_millis(300);
    let started = Instant::now();
    let err = speech::transcribe(&program, &req).unwrap_err();
    assert!(matches!(err, ExternalError::Timeout { .. }), "{err}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_cancelled_run_kills_the_program() {
    let tmp = tempfile::tempdir().unwrap();
    let marker = tmp.path().join("survived");
    let program = script(
        tmp.path(),
        "slow",
        &format!("sleep 2; touch '{}'", marker.display()),
    );
    let cancel = AtomicBool::new(true);
    let limits = Limits {
        timeout: Duration::from_secs(20),
        max_stdout: 1024,
        max_stderr: 1024,
        cancel: Some(&cancel),
    };
    let err = run(&program, &[], &limits).unwrap_err();
    assert!(matches!(err, ExternalError::Cancelled { .. }), "{err}");
    std::thread::sleep(Duration::from_millis(2500));
    assert!(!marker.exists(), "the child was killed before it finished");
}

#[test]
fn output_over_the_cap_stops_the_program_and_error_text_is_capped() {
    let tmp = tempfile::tempdir().unwrap();
    let flood = script(tmp.path(), "flood", "yes");
    let limits = Limits {
        timeout: Duration::from_secs(20),
        max_stdout: 10_000,
        max_stderr: 100,
        cancel: None,
    };
    let err = run(&flood, &[], &limits).unwrap_err();
    assert!(
        matches!(err, ExternalError::OutputTooLarge { limit: 10_000, .. }),
        "{err}"
    );

    let noisy = script(tmp.path(), "noisy", "yes x | head -c 100000 >&2; exit 3");
    let done = run(&noisy, &[], &limits).unwrap();
    assert!(!done.success);
    assert!(
        done.stderr.len() <= 100,
        "stderr is kept only up to its cap"
    );
}

#[test]
fn arguments_are_never_interpreted_by_a_shell() {
    let tmp = tempfile::tempdir().unwrap();
    let echo = script(tmp.path(), "echo-args", "printf '%s|' \"$@\"");
    let limits = Limits {
        timeout: Duration::from_secs(5),
        max_stdout: 1024,
        max_stderr: 1024,
        cancel: None,
    };
    let args: Vec<OsString> = vec!["$(touch pwned)".into(), "; rm -rf /".into(), "a b".into()];
    let done = run(&echo, &args, &limits).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&done.stdout),
        "$(touch pwned)|; rm -rf /|a b|"
    );
    assert!(!std::path::Path::new("pwned").exists());
}

#[test]
fn a_failing_program_reports_its_last_error_line_and_a_missing_result_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let model = Model::File(tmp.path().join("m.bin"));
    let scratch = Scratch::new().unwrap();
    let wav = scratch.path().join("in.wav");
    let failing = Whisper {
        path: script(tmp.path(), "whisper-cli", "echo 'bad model' >&2; exit 1"),
        flavor: WhisperFlavor::Cpp,
    };
    let err = speech::transcribe(&failing, &request(&wav, &model, scratch.path())).unwrap_err();
    assert!(err.to_string().contains("bad model"), "{err}");

    let silent = Whisper {
        path: script(tmp.path(), "whisper-cli", "exit 0"),
        flavor: WhisperFlavor::Cpp,
    };
    let err = speech::transcribe(&silent, &request(&wav, &model, scratch.path())).unwrap_err();
    assert!(err.to_string().contains("no result"), "{err}");
}

#[test]
fn the_scratch_folder_is_private_and_removed_and_the_project_is_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    std::fs::create_dir_all(project.join("media")).unwrap();
    let before = std::fs::read_dir(&project).unwrap().count();
    let program = Whisper {
        path: fake_cpp(tmp.path(), "whisper-cli"),
        flavor: WhisperFlavor::Cpp,
    };
    let model = Model::File(tmp.path().join("m.bin"));
    let scratch_path;
    {
        let scratch = Scratch::new().unwrap();
        scratch_path = scratch.path().to_path_buf();
        let wav = scratch.path().join("in.wav");
        speech::transcribe(&program, &request(&wav, &model, scratch.path())).unwrap();
        assert!(scratch_path.join("out.json").is_file());
        let mode = std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(&scratch_path).unwrap().permissions(),
        );
        assert_eq!(mode & 0o077, 0, "only the user can read the scratch folder");
    }
    assert!(!scratch_path.exists());
    assert_eq!(std::fs::read_dir(&project).unwrap().count(), before);
    assert_eq!(std::fs::read_dir(project.join("media")).unwrap().count(), 0);
}

#[test]
fn each_json_format_is_parsed_and_broken_ones_are_errors() {
    let cpp = speech::parse_cpp_json(CPP_JSON).unwrap();
    let openai = speech::parse_openai_json(OPENAI_JSON).unwrap();
    assert_eq!(cpp, openai, "both formats give the same words");

    assert!(speech::parse_cpp_json("not json").is_err());
    assert!(speech::parse_cpp_json(r#"{"transcription": [{"text": "x"}]}"#).is_err());
    assert!(
        speech::parse_openai_json(r#"{"segments": [{"text": "no words"}]}"#)
            .unwrap_err()
            .to_string()
            .contains("word timings")
    );
    assert!(
        speech::parse_openai_json(
            r#"{"segments": [{"words": [{"word": "x", "start": -1, "end": 2}]}]}"#
        )
        .is_err()
    );
}

#[test]
fn the_words_feed_gaps_and_the_caption_importer() {
    let words = speech::parse_cpp_json(CPP_JSON).unwrap();
    assert_eq!(speech::gaps(&words, 500), vec![(950, 2000)]);
    let json = speech::words_json(&words);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed[0]["text"], "Hello");
    assert_eq!(parsed[2]["start_ms"], 2000);
}
