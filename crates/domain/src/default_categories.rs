//! The curated category tree a brand-new database is seeded with, in every
//! language the app ships.
//!
//! This lives in `domain` rather than next to the code that writes it
//! (`scrat_infra_sqlite::seed`) because two layers need it and neither may
//! depend on the other: the SQLite adapter inserts it on `create_new`, and
//! `CategoryService::relabel_seeded_categories` reads it to rename untouched
//! categories when the interface language changes. It is pure data with no
//! I/O, so `domain` is where both can reach it.
//!
//! ## Why a stable key exists at all
//!
//! A seeded category is app-owned right up until the user touches it, and
//! then it is theirs. The key is what tells those two states apart across a
//! rename: "the category the app created as Housing" survives the app
//! relabelling it to `Logement`, whereas matching on the name would lose track
//! of it the moment it changed — which is precisely when the answer matters.
//!
//! Keys are namespaced by their parent (`housing.rent`) because subcategory
//! names are not unique on their own: `Insurance > Travel` and the top-level
//! `Travel` are different categories that happen to share a word, and a flat
//! key space would conflate them.
//!
//! **Keys are storage identifiers. Never renumber or reword one** — a shipped
//! key is written into every user's `categories.seed_key` column. Adding a new
//! entry is fine; changing an existing key orphans the row it used to name.

use crate::language::Language;

/// A top-level seeded category: its stable key, its icon, and its name in
/// each language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultCategory {
    pub key: &'static str,
    pub icon: &'static str,
    pub en: &'static str,
    pub fr: &'static str,
    pub children: &'static [DefaultSubcategory],
}

/// A seeded subcategory. Carries no icon: icons are a top-level-only concept
/// (see `Category::set_icon`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultSubcategory {
    pub key: &'static str,
    pub en: &'static str,
    pub fr: &'static str,
}

impl DefaultCategory {
    pub fn name(&self, language: Language) -> &'static str {
        match language {
            Language::En => self.en,
            Language::Fr => self.fr,
        }
    }
}

impl DefaultSubcategory {
    pub fn name(&self, language: Language) -> &'static str {
        match language {
            Language::En => self.en,
            Language::Fr => self.fr,
        }
    }
}

/// The key of the forced fallback category — see
/// [`crate::category::DEFAULT_CATEGORY_NAME`]. Identifying it by key rather
/// than by name is what lets it be called `Non classé` in French and still be
/// recognised as the one category that cannot be renamed or deleted.
pub const UNCATEGORIZED_KEY: &str = "uncategorized";

/// Expense categories, then income, then transfers — one flat list, since
/// nothing downstream distinguishes them. (Direction is derived from a
/// transaction's amount sign, not from its category; see `Direction` in
/// `transaction.rs`.) The grouping survives only as ordering and comments.
pub const DEFAULT_CATEGORIES: &[DefaultCategory] = &[
    // ---- Expenses ----
    DefaultCategory {
        key: "food_and_drink",
        icon: "utensils",
        en: "Food & Drink",
        fr: "Nourriture & boissons",
        children: &[
            DefaultSubcategory {
                key: "food_and_drink.restaurant",
                en: "Restaurant",
                fr: "Restaurant",
            },
            DefaultSubcategory {
                key: "food_and_drink.bar",
                en: "Bar",
                fr: "Bar",
            },
        ],
    },
    DefaultCategory {
        key: "groceries",
        icon: "shopping-cart",
        en: "Groceries",
        fr: "Courses",
        children: &[],
    },
    DefaultCategory {
        key: "housing",
        icon: "house",
        en: "Housing",
        fr: "Logement",
        children: &[
            DefaultSubcategory {
                key: "housing.rent",
                en: "Rent",
                fr: "Loyer",
            },
            DefaultSubcategory {
                key: "housing.mortgage",
                en: "Mortgage",
                fr: "Prêt immobilier",
            },
            DefaultSubcategory {
                key: "housing.maintenance",
                en: "Maintenance",
                fr: "Entretien",
            },
            DefaultSubcategory {
                key: "housing.furniture",
                en: "Furniture",
                fr: "Mobilier",
            },
            DefaultSubcategory {
                key: "housing.appliances",
                en: "Appliances",
                fr: "Électroménager",
            },
            DefaultSubcategory {
                key: "housing.home_decor",
                en: "Home Decor",
                fr: "Décoration",
            },
        ],
    },
    DefaultCategory {
        key: "utilities",
        icon: "plug",
        en: "Utilities",
        fr: "Charges",
        children: &[
            DefaultSubcategory {
                key: "utilities.electricity",
                en: "Electricity",
                fr: "Électricité",
            },
            DefaultSubcategory {
                key: "utilities.water",
                en: "Water",
                fr: "Eau",
            },
            DefaultSubcategory {
                key: "utilities.gas",
                en: "Gas",
                fr: "Gaz",
            },
            DefaultSubcategory {
                key: "utilities.internet",
                en: "Internet",
                fr: "Internet",
            },
            DefaultSubcategory {
                key: "utilities.mobile_phone",
                en: "Mobile Phone",
                fr: "Téléphone mobile",
            },
            DefaultSubcategory {
                key: "utilities.tv_and_streaming",
                en: "TV & Streaming",
                fr: "TV & streaming",
            },
        ],
    },
    DefaultCategory {
        key: "transportation",
        icon: "car",
        en: "Transportation",
        fr: "Transport",
        children: &[
            DefaultSubcategory {
                key: "transportation.fuel",
                en: "Fuel",
                fr: "Carburant",
            },
            DefaultSubcategory {
                key: "transportation.public_transit",
                en: "Public Transit",
                fr: "Transports en commun",
            },
            DefaultSubcategory {
                key: "transportation.taxi_and_rideshare",
                en: "Taxi & Rideshare",
                fr: "Taxi & VTC",
            },
            DefaultSubcategory {
                key: "transportation.parking",
                en: "Parking",
                fr: "Stationnement",
            },
            DefaultSubcategory {
                key: "transportation.tolls",
                en: "Tolls",
                fr: "Péages",
            },
            DefaultSubcategory {
                key: "transportation.vehicle_maintenance",
                en: "Vehicle Maintenance",
                fr: "Entretien du véhicule",
            },
        ],
    },
    DefaultCategory {
        key: "healthcare",
        icon: "heart-pulse",
        en: "Healthcare",
        fr: "Santé",
        children: &[
            DefaultSubcategory {
                key: "healthcare.doctor",
                en: "Doctor",
                fr: "Médecin",
            },
            DefaultSubcategory {
                key: "healthcare.pharmacy",
                en: "Pharmacy",
                fr: "Pharmacie",
            },
        ],
    },
    DefaultCategory {
        key: "personal_care",
        icon: "sparkles",
        en: "Personal Care",
        fr: "Soins personnels",
        children: &[
            DefaultSubcategory {
                key: "personal_care.haircuts",
                en: "Haircuts",
                fr: "Coiffure",
            },
            DefaultSubcategory {
                key: "personal_care.cosmetics",
                en: "Cosmetics",
                fr: "Cosmétiques",
            },
            DefaultSubcategory {
                key: "personal_care.skincare",
                en: "Skincare",
                fr: "Soins de la peau",
            },
            DefaultSubcategory {
                key: "personal_care.hygiene",
                en: "Hygiene",
                fr: "Hygiène",
            },
        ],
    },
    // "Clothing" and its "Clothes" child both translate to *vêtements* on
    // their own; the parent takes the broader *Habillement* so the pair reads
    // as two things in French the way it does in English.
    DefaultCategory {
        key: "clothing",
        icon: "shirt",
        en: "Clothing",
        fr: "Habillement",
        children: &[
            DefaultSubcategory {
                key: "clothing.clothes",
                en: "Clothes",
                fr: "Vêtements",
            },
            DefaultSubcategory {
                key: "clothing.shoes",
                en: "Shoes",
                fr: "Chaussures",
            },
            DefaultSubcategory {
                key: "clothing.accessories",
                en: "Accessories",
                fr: "Accessoires",
            },
        ],
    },
    DefaultCategory {
        key: "entertainment",
        icon: "film",
        en: "Entertainment",
        fr: "Loisirs",
        children: &[
            DefaultSubcategory {
                key: "entertainment.movies",
                en: "Movies",
                fr: "Cinéma",
            },
            DefaultSubcategory {
                key: "entertainment.concerts",
                en: "Concerts",
                fr: "Concerts",
            },
            DefaultSubcategory {
                key: "entertainment.games",
                en: "Games",
                fr: "Jeux",
            },
            DefaultSubcategory {
                key: "entertainment.hobbies",
                en: "Hobbies",
                fr: "Passe-temps",
            },
            DefaultSubcategory {
                key: "entertainment.events",
                en: "Events",
                fr: "Événements",
            },
        ],
    },
    DefaultCategory {
        key: "sports_and_fitness",
        icon: "dumbbell",
        en: "Sports & Fitness",
        fr: "Sport & forme",
        children: &[DefaultSubcategory {
            key: "sports_and_fitness.gym",
            en: "Gym",
            fr: "Salle de sport",
        }],
    },
    DefaultCategory {
        key: "education",
        icon: "graduation-cap",
        en: "Education",
        fr: "Éducation",
        children: &[
            DefaultSubcategory {
                key: "education.books",
                en: "Books",
                fr: "Livres",
            },
            // "Courses" here is the English plural of *a course*, not the
            // French *courses* (groceries) — hence "Formations".
            DefaultSubcategory {
                key: "education.courses",
                en: "Courses",
                fr: "Formations",
            },
        ],
    },
    DefaultCategory {
        key: "travel",
        icon: "plane",
        en: "Travel",
        fr: "Voyages",
        children: &[
            DefaultSubcategory {
                key: "travel.flights",
                en: "Flights",
                fr: "Vols",
            },
            DefaultSubcategory {
                key: "travel.accommodation",
                en: "Accommodation",
                fr: "Hébergement",
            },
            DefaultSubcategory {
                key: "travel.trains",
                en: "Trains",
                fr: "Trains",
            },
            DefaultSubcategory {
                key: "travel.car_rental",
                en: "Car Rental",
                fr: "Location de voiture",
            },
            DefaultSubcategory {
                key: "travel.activities",
                en: "Activities",
                fr: "Activités",
            },
        ],
    },
    DefaultCategory {
        key: "gifts_and_donations",
        icon: "gift",
        en: "Gifts & Donations",
        fr: "Cadeaux & dons",
        children: &[DefaultSubcategory {
            key: "gifts_and_donations.gifts",
            en: "Gifts",
            fr: "Cadeaux",
        }],
    },
    DefaultCategory {
        key: "financial",
        icon: "landmark",
        en: "Financial",
        fr: "Frais financiers",
        children: &[
            DefaultSubcategory {
                key: "financial.bank_fees",
                en: "Bank Fees",
                fr: "Frais bancaires",
            },
            DefaultSubcategory {
                key: "financial.loan_payments",
                en: "Loan Payments",
                fr: "Remboursements de prêt",
            },
        ],
    },
    DefaultCategory {
        key: "taxes_and_government",
        icon: "receipt",
        en: "Taxes & Government",
        fr: "Impôts & administration",
        children: &[
            DefaultSubcategory {
                key: "taxes_and_government.income_tax",
                en: "Income Tax",
                fr: "Impôt sur le revenu",
            },
            DefaultSubcategory {
                key: "taxes_and_government.property_tax",
                en: "Property Tax",
                fr: "Taxe foncière",
            },
            DefaultSubcategory {
                key: "taxes_and_government.vehicle_tax",
                en: "Vehicle Tax",
                fr: "Taxe sur les véhicules",
            },
            DefaultSubcategory {
                key: "taxes_and_government.government_fees",
                en: "Government Fees",
                fr: "Frais administratifs",
            },
        ],
    },
    DefaultCategory {
        key: "insurance",
        icon: "shield",
        en: "Insurance",
        fr: "Assurances",
        children: &[
            DefaultSubcategory {
                key: "insurance.health",
                en: "Health",
                fr: "Santé",
            },
            DefaultSubcategory {
                key: "insurance.home",
                en: "Home",
                fr: "Habitation",
            },
            DefaultSubcategory {
                key: "insurance.vehicle",
                en: "Vehicle",
                fr: "Véhicule",
            },
            DefaultSubcategory {
                key: "insurance.life",
                en: "Life",
                fr: "Vie",
            },
            DefaultSubcategory {
                key: "insurance.travel",
                en: "Travel",
                fr: "Voyage",
            },
        ],
    },
    DefaultCategory {
        key: UNCATEGORIZED_KEY,
        icon: "circle-question-mark",
        en: "Uncategorized",
        fr: "Non classé",
        children: &[],
    },
    // ---- Income ----
    DefaultCategory {
        key: "salary",
        icon: "briefcase",
        en: "Salary",
        fr: "Salaire",
        children: &[
            DefaultSubcategory {
                key: "salary.base_salary",
                en: "Base Salary",
                fr: "Salaire de base",
            },
            DefaultSubcategory {
                key: "salary.overtime",
                en: "Overtime",
                fr: "Heures supplémentaires",
            },
            DefaultSubcategory {
                key: "salary.commission",
                en: "Commission",
                fr: "Commissions",
            },
        ],
    },
    DefaultCategory {
        key: "bonus",
        icon: "award",
        en: "Bonus",
        fr: "Primes",
        children: &[
            DefaultSubcategory {
                key: "bonus.performance",
                en: "Performance",
                fr: "Prime de performance",
            },
            DefaultSubcategory {
                key: "bonus.holiday",
                en: "Holiday",
                fr: "Prime de vacances",
            },
            DefaultSubcategory {
                key: "bonus.referral",
                en: "Referral",
                fr: "Prime de parrainage",
            },
        ],
    },
    DefaultCategory {
        key: "freelance_and_business",
        icon: "laptop",
        en: "Freelance & Business",
        fr: "Freelance & entreprise",
        children: &[
            DefaultSubcategory {
                key: "freelance_and_business.client_payments",
                en: "Client Payments",
                fr: "Paiements clients",
            },
            DefaultSubcategory {
                key: "freelance_and_business.product_sales",
                en: "Product Sales",
                fr: "Ventes de produits",
            },
            DefaultSubcategory {
                key: "freelance_and_business.service_income",
                en: "Service Income",
                fr: "Prestations de services",
            },
        ],
    },
    DefaultCategory {
        key: "investment_income",
        icon: "trending-up",
        en: "Investment Income",
        fr: "Revenus de placements",
        children: &[
            DefaultSubcategory {
                key: "investment_income.dividends",
                en: "Dividends",
                fr: "Dividendes",
            },
            DefaultSubcategory {
                key: "investment_income.interest",
                en: "Interest",
                fr: "Intérêts",
            },
            DefaultSubcategory {
                key: "investment_income.capital_gains",
                en: "Capital Gains",
                fr: "Plus-values",
            },
        ],
    },
    DefaultCategory {
        key: "rental_income",
        icon: "building",
        en: "Rental Income",
        fr: "Revenus locatifs",
        children: &[DefaultSubcategory {
            key: "rental_income.property_rent",
            en: "Property Rent",
            fr: "Loyers perçus",
        }],
    },
    DefaultCategory {
        key: "government_benefits",
        icon: "landmark",
        en: "Government Benefits",
        fr: "Prestations sociales",
        children: &[
            DefaultSubcategory {
                key: "government_benefits.pension",
                en: "Pension",
                fr: "Retraite",
            },
            DefaultSubcategory {
                key: "government_benefits.unemployment",
                en: "Unemployment",
                fr: "Chômage",
            },
            DefaultSubcategory {
                key: "government_benefits.child_benefits",
                en: "Child Benefits",
                fr: "Allocations familiales",
            },
            DefaultSubcategory {
                key: "government_benefits.social_assistance",
                en: "Social Assistance",
                fr: "Aide sociale",
            },
        ],
    },
    DefaultCategory {
        key: "refunds_and_reimbursements",
        icon: "rotate-ccw",
        en: "Refunds & Reimbursements",
        fr: "Remboursements",
        children: &[
            DefaultSubcategory {
                key: "refunds_and_reimbursements.purchase_refund",
                en: "Purchase Refund",
                fr: "Remboursement d'achat",
            },
            DefaultSubcategory {
                key: "refunds_and_reimbursements.tax_refund",
                en: "Tax Refund",
                fr: "Remboursement d'impôt",
            },
            DefaultSubcategory {
                key: "refunds_and_reimbursements.insurance_reimbursement",
                en: "Insurance Reimbursement",
                fr: "Remboursement d'assurance",
            },
            DefaultSubcategory {
                key: "refunds_and_reimbursements.expense_reimbursement",
                en: "Expense Reimbursement",
                fr: "Note de frais",
            },
        ],
    },
    // ---- Transfers ----
    DefaultCategory {
        key: "transfers",
        icon: "arrow-left-right",
        en: "Transfers",
        fr: "Virements",
        children: &[
            DefaultSubcategory {
                key: "transfers.investment_transfers",
                en: "Investment Transfers",
                fr: "Virements vers placements",
            },
            DefaultSubcategory {
                key: "transfers.credit_card_payments",
                en: "Credit Card Payments",
                fr: "Paiements carte de crédit",
            },
            DefaultSubcategory {
                key: "transfers.cash_withdrawal",
                en: "Cash Withdrawal",
                fr: "Retrait d'espèces",
            },
            DefaultSubcategory {
                key: "transfers.cash_deposit",
                en: "Cash Deposit",
                fr: "Dépôt d'espèces",
            },
        ],
    },
];

/// The name a seeded key carries in `language`, or `None` if the key isn't one
/// this build ships. An unknown key is a real possibility, not a bug: a
/// database written by a newer version can carry keys this binary has never
/// heard of, and the honest answer is to leave that category's name alone.
pub fn seeded_name(key: &str, language: Language) -> Option<&'static str> {
    for parent in DEFAULT_CATEGORIES {
        if parent.key == key {
            return Some(parent.name(language));
        }
        for child in parent.children {
            if child.key == key {
                return Some(child.name(language));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn every_key() -> Vec<&'static str> {
        DEFAULT_CATEGORIES
            .iter()
            .flat_map(|parent| {
                std::iter::once(parent.key).chain(parent.children.iter().map(|child| child.key))
            })
            .collect()
    }

    /// Keys are the identity of a seeded row in the database. A duplicate
    /// would make two different categories indistinguishable to the relabel
    /// pass, which would then rename both to the same thing.
    #[test]
    fn every_seed_key_is_unique() {
        let keys = every_key();
        let unique: HashSet<_> = keys.iter().collect();
        assert_eq!(
            unique.len(),
            keys.len(),
            "duplicate seed key in DEFAULT_CATEGORIES"
        );
    }

    /// A subcategory's key is namespaced by its parent's. Without this,
    /// `Insurance > Travel` and the top-level `Travel` would collide.
    #[test]
    fn every_subcategory_key_is_namespaced_by_its_parent() {
        for parent in DEFAULT_CATEGORIES {
            for child in parent.children {
                assert!(
                    child.key.starts_with(&format!("{}.", parent.key)),
                    "'{}' should be namespaced under '{}'",
                    child.key,
                    parent.key
                );
            }
        }
    }

    /// A missing translation would seed (or relabel to) an empty name, which
    /// `CategoryName::new` rejects — better to catch it here than as an
    /// `expect` panic on a user's first run.
    #[test]
    fn every_category_has_a_name_in_every_language() {
        for language in Language::ALL {
            for parent in DEFAULT_CATEGORIES {
                assert!(
                    !parent.name(language).trim().is_empty(),
                    "'{}' has no {} name",
                    parent.key,
                    language.as_str()
                );
                for child in parent.children {
                    assert!(
                        !child.name(language).trim().is_empty(),
                        "'{}' has no {} name",
                        child.key,
                        language.as_str()
                    );
                }
            }
        }
    }

    /// Icons are validated by `CategoryIcon::new`, which seeding calls with
    /// `expect` — an unknown key here would panic on first run rather than
    /// fail a test.
    #[test]
    fn every_icon_is_a_known_icon_key() {
        for parent in DEFAULT_CATEGORIES {
            assert!(
                crate::category::CATEGORY_ICONS.contains(&parent.icon),
                "'{}' uses unknown icon '{}'",
                parent.key,
                parent.icon
            );
        }
    }

    /// The English names are what migration 0010 matches on to backfill
    /// `seed_key` in databases created before the column existed. If one is
    /// reworded here without a matching new migration, existing users' rows
    /// silently stop being recognised as seeded.
    #[test]
    fn the_uncategorized_entry_keeps_its_english_name_and_key() {
        let uncategorized = DEFAULT_CATEGORIES
            .iter()
            .find(|c| c.key == UNCATEGORIZED_KEY)
            .expect("the fallback category must be in the seed list");
        assert_eq!(uncategorized.en, crate::category::DEFAULT_CATEGORY_NAME);
        assert!(uncategorized.children.is_empty());
    }

    #[test]
    fn seeded_name_resolves_parents_and_children_in_both_languages() {
        assert_eq!(seeded_name("housing", Language::En), Some("Housing"));
        assert_eq!(seeded_name("housing", Language::Fr), Some("Logement"));
        assert_eq!(seeded_name("housing.rent", Language::En), Some("Rent"));
        assert_eq!(seeded_name("housing.rent", Language::Fr), Some("Loyer"));
    }

    /// A key from a newer build is data, not a crash: the caller leaves that
    /// category's name untouched.
    #[test]
    fn seeded_name_returns_none_for_an_unknown_key() {
        assert_eq!(seeded_name("crypto.staking_rewards", Language::En), None);
        assert_eq!(seeded_name("", Language::Fr), None);
    }
}
