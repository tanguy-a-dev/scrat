//! Recognizing the payment instrument a bank export names.
//!
//! Banks write the same handful of instruments a dozen ways — "Carte
//! bancaire", "CARTE", "CB", "Paiement CB", "Card payment" — across
//! languages, casing, accents and abbreviations. This maps whatever a file
//! says onto the domain's closed [`OperationKind`] set.
//!
//! It is a *heuristic over free text*, so the same caution the rest of this
//! crate applies to external input applies here: a merchant name can contain
//! a keyword by coincidence. Two things keep that from mattering much — short
//! abbreviations (`cb`, `vir`, `prlv`) only ever match as whole tokens, never
//! as substrings, so "VIRGIN MEDIA" is not a wire transfer; and the result is
//! descriptive only. Nothing about a row's amount, category, or whether it
//! counts as spending depends on it.

use scrat_domain::transaction::OperationKind;

use crate::detection::fold;

/// Instrument vocabulary, most specific first — the first entry that matches
/// wins, which is why [`OperationKind::Card`] is last: "Retrait carte" is a
/// cash withdrawal and "Frais carte bancaire" is a bank charge, even though
/// both name a card.
///
/// The two keyword lists differ in how they match, and the split is
/// load-bearing. `substrings` are long enough to be safe inside a larger
/// word ("virement", "prelevement"). `tokens` are the short abbreviations,
/// matched only as whole words — `vir` as a substring would find "VIRGIN",
/// and `cb` would find any merchant with those two letters adjacent.
const VOCABULARY: &[(OperationKind, &[&str], &[&str])] = &[
    (
        OperationKind::Fees,
        &["frais", "commission", "agios", "cotisation"],
        &["fee", "fees"],
    ),
    (OperationKind::Check, &["cheque"], &["chq", "check"]),
    (
        OperationKind::Cash,
        &["retrait", "especes", "distributeur", "withdrawal"],
        &["dab", "atm", "cash"],
    ),
    (
        OperationKind::DirectDebit,
        &["prelevement", "direct debit"],
        &["prlv", "prel"],
    ),
    (
        OperationKind::BankTransfer,
        &["virement", "transfer"],
        &["vir", "wire"],
    ),
    (OperationKind::Card, &["carte", "card"], &["cb"]),
];

/// Splits on everything that isn't alphanumeric, so "VIR." and "VIR/SEPA"
/// both yield the token `vir` while "VIRGIN" stays a single token that no
/// abbreviation matches.
fn tokens(folded: &str) -> Vec<&str> {
    folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Reads a bank's own operation-type label (the "Type opération" cell).
///
/// `None` means the cell was blank — the caller has learned nothing and
/// should fall back to [`from_description`]. `Some(OperationKind::Other)`
/// means the cell said *something* this vocabulary doesn't know, which is
/// real information: better to record "an instrument we don't have a name
/// for" than to quietly file it as a card payment.
pub fn from_label(raw: &str) -> Option<OperationKind> {
    if raw.trim().is_empty() {
        return None;
    }
    Some(recognize(raw).unwrap_or(OperationKind::Other))
}

/// Falls back to the row's description text when the file has no
/// operation-type column (or left the cell blank) — the common case, since
/// plenty of exports carry the instrument inline in the description instead
/// ("CB SOME STORE", "VIR SEPA EMPLOYER", "PRLV SEPA UTILITY").
///
/// Anything unrecognized is a card payment, not `Other`: a description that
/// is simply a merchant name is no evidence of an unusual instrument, and
/// card is the likeliest thing an export that never said means.
pub fn from_description(description: &str) -> OperationKind {
    recognize(description).unwrap_or(OperationKind::Card)
}

fn recognize(text: &str) -> Option<OperationKind> {
    let folded = fold(text);
    let tokens = tokens(&folded);
    VOCABULARY
        .iter()
        .find(|(_, substrings, abbreviations)| {
            substrings.iter().any(|s| folded.contains(s))
                || abbreviations.iter().any(|a| tokens.contains(a))
        })
        .map(|(kind, _, _)| *kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_labels_a_french_export_uses() {
        for (label, expected) in [
            ("Carte bancaire", OperationKind::Card),
            ("Carte", OperationKind::Card),
            ("CB", OperationKind::Card),
            ("Virement", OperationKind::BankTransfer),
            ("Virement reçu", OperationKind::BankTransfer),
            ("Frais bancaires", OperationKind::Fees),
            ("Prélèvement", OperationKind::DirectDebit),
            ("Chèque", OperationKind::Check),
            ("Retrait DAB", OperationKind::Cash),
        ] {
            assert_eq!(from_label(label), Some(expected), "label: {label}");
        }
    }

    #[test]
    fn reads_the_labels_an_english_export_uses() {
        for (label, expected) in [
            ("Card payment", OperationKind::Card),
            ("Transfer", OperationKind::BankTransfer),
            ("Wire transfer", OperationKind::BankTransfer),
            ("Direct debit", OperationKind::DirectDebit),
            ("Cheque", OperationKind::Check),
            ("ATM withdrawal", OperationKind::Cash),
            ("Account fees", OperationKind::Fees),
        ] {
            assert_eq!(from_label(label), Some(expected), "label: {label}");
        }
    }

    #[test]
    fn a_blank_label_says_nothing_rather_than_defaulting() {
        assert_eq!(from_label(""), None);
        assert_eq!(from_label("   "), None);
    }

    /// A label the vocabulary doesn't know is still evidence that the bank
    /// named *some* instrument — recording that beats claiming a card
    /// payment the file never mentioned.
    #[test]
    fn an_unrecognized_label_is_other_not_card() {
        assert_eq!(from_label("Escompte"), Some(OperationKind::Other));
    }

    /// The whole reason short abbreviations are matched as tokens. Every one
    /// of these descriptions embeds an abbreviation inside a longer word;
    /// none of them is that instrument.
    #[test]
    fn abbreviations_inside_a_longer_word_are_not_matched() {
        for description in [
            "VIRGIN MEDIA LTD",
            "ENVIRONNEMENT SERVICES",
            "PRELUDE BOUTIQUE",
            "ATMOSPHERE CAFE",
        ] {
            assert_eq!(
                from_description(description),
                OperationKind::Card,
                "description: {description}"
            );
        }
    }

    /// …while the abbreviation as its own token, with or without the
    /// trailing dot a French statement writes, is.
    #[test]
    fn transfer_abbreviations_are_matched_as_whole_tokens() {
        for description in [
            "VIR. EMPLOYER SALARY",
            "VIR SEPA RECU M DUPONT",
            "VIREMENT EN VOTRE FAVEUR",
            "WIRE FROM ACME LTD",
            "INCOMING TRANSFER",
        ] {
            assert_eq!(
                from_description(description),
                OperationKind::BankTransfer,
                "description: {description}"
            );
        }
    }

    #[test]
    fn a_description_naming_no_instrument_is_a_card_payment() {
        assert_eq!(from_description("SC-SUSHI SASHI"), OperationKind::Card);
        assert_eq!(from_description(""), OperationKind::Card);
    }

    /// Most specific wins: a withdrawal made with a card is cash out, and a
    /// charge levied on a card is a fee — neither is a card *payment*.
    #[test]
    fn a_label_naming_two_instruments_resolves_to_the_more_specific_one() {
        assert_eq!(from_label("Retrait carte"), Some(OperationKind::Cash));
        assert_eq!(
            from_label("Frais carte bancaire"),
            Some(OperationKind::Fees)
        );
    }

    #[test]
    fn accents_and_casing_do_not_change_the_result() {
        assert_eq!(from_label("PRÉLÈVEMENT"), from_label("prelevement"));
        assert_eq!(from_label("Chèque"), from_label("CHEQUE"));
        assert_eq!(from_label("VIREMENT REÇU"), from_label("virement recu"));
    }
}
