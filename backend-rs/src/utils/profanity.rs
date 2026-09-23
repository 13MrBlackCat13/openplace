use regex::Regex;
use std::sync::OnceLock;

/// Approximation of the JS `obscenity` englishDataset matcher: whole-word
/// matching over a core list of English profanity/slurs. Kept short on
/// purpose; extend the list if stricter moderation is required.
const WORDS: &[&str] = &[
    "anal",
    "anilingus",
    "anus",
    "arse",
    "asshat",
    "asshole",
    "bastard",
    "bitch",
    "bollocks",
    "boner",
    "boob",
    "bullshit",
    "buttcheek",
    "clit",
    "cock",
    "coochie",
    "cooter",
    "cum",
    "cunt",
    "dick",
    "dildo",
    "douche",
    "duchebag",
    "dyke",
    "fag",
    "faggot",
    "fuck",
    "goddamn",
    "handjob",
    "hoe",
    "homo",
    "horny",
    "jerk off",
    "jizz",
    "knob end",
    "labia",
    "milf",
    "nigga",
    "nigger",
    "niglet",
    "penis",
    "porn",
    "prick",
    "pube",
    "pussies",
    "pussy",
    "queef",
    "queer",
    "rape",
    "rapist",
    "retard",
    "scrotum",
    "semen",
    "sex",
    "shit",
    "slut",
    "smut",
    "spunk",
    "tit",
    "twat",
    "vagina",
    "wank",
    "whore",
];

fn matcher() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let mut pattern = String::from("(?i)\\b(?:");
        for (i, w) in WORDS.iter().enumerate() {
            if i > 0 {
                pattern.push('|');
            }
            pattern.push_str(&regex::escape(w));
        }
        pattern.push_str(")\\b");
        Regex::new(&pattern).unwrap()
    })
}

pub fn is_acceptable_username(name: &str) -> bool {
    !matcher().is_match(name)
}
