//! Default category tree created once, when a brand-new database is set up
//! (see `connection::create_new`) — never reapplied on `unlock_existing`, so
//! renaming or deleting a seeded category sticks.

use rusqlite::Connection;
use scrat_domain::category::{Category, CategoryId, CategoryName};
use scrat_domain::ports::{CategoryRepository, RepositoryError};

use crate::category_repository::SqliteCategoryRepository;

type CategoryTree = &'static [(&'static str, &'static [&'static str])];

const EXPENSE_CATEGORIES: CategoryTree = &[
    (
        "Food & Drink",
        &[
            "Restaurants",
            "Fast Food",
            "Cafés",
            "Bars",
            "Delivery",
            "Snacks",
        ],
    ),
    (
        "Groceries",
        &["Supermarket", "Farmers Market", "Household Supplies"],
    ),
    (
        "Housing",
        &[
            "Rent",
            "Mortgage",
            "HOA Fees",
            "Maintenance",
            "Furniture",
            "Appliances",
            "Home Decor",
        ],
    ),
    (
        "Utilities",
        &[
            "Electricity",
            "Water",
            "Gas",
            "Internet",
            "Mobile Phone",
            "TV & Streaming",
            "Waste Collection",
        ],
    ),
    (
        "Transportation",
        &[
            "Fuel",
            "Public Transit",
            "Taxi & Rideshare",
            "Parking",
            "Tolls",
            "Vehicle Maintenance",
            "Vehicle Insurance",
            "Registration",
        ],
    ),
    (
        "Healthcare",
        &[
            "Doctor",
            "Pharmacy",
            "Dental",
            "Vision",
            "Therapy",
            "Health Insurance",
            "Medical Equipment",
        ],
    ),
    (
        "Personal Care",
        &["Haircuts", "Cosmetics", "Skincare", "Hygiene"],
    ),
    ("Clothing", &["Clothes", "Shoes", "Accessories"]),
    (
        "Entertainment",
        &[
            "Movies",
            "Concerts",
            "Games",
            "Hobbies",
            "Events",
            "Streaming Services",
        ],
    ),
    (
        "Sports & Fitness",
        &["Gym", "Sports Club", "Equipment", "Classes"],
    ),
    (
        "Education",
        &["Books", "Courses", "Tuition", "Certifications", "Supplies"],
    ),
    (
        "Travel",
        &[
            "Flights",
            "Accommodation",
            "Trains",
            "Car Rental",
            "Activities",
            "Travel Insurance",
        ],
    ),
    ("Gifts & Donations", &["Gifts", "Charity", "Tips"]),
    (
        "Financial",
        &[
            "Bank Fees",
            "Loan Payments",
            "Credit Card Interest",
            "Investment Contributions",
            "Investment Fees",
        ],
    ),
    (
        "Taxes & Government",
        &[
            "Income Tax",
            "Property Tax",
            "Vehicle Tax",
            "Government Fees",
        ],
    ),
    (
        "Insurance",
        &["Health", "Home", "Vehicle", "Life", "Travel"],
    ),
    ("Pets", &["Food", "Veterinary", "Grooming", "Supplies"]),
    (
        "Children & Family",
        &["Childcare", "School", "Activities", "Allowance"],
    ),
    (
        "Business",
        &[
            "Office Supplies",
            "Software",
            "Professional Services",
            "Business Travel",
        ],
    ),
    ("Miscellaneous", &["Uncategorized"]),
];

const INCOME_CATEGORIES: CategoryTree = &[
    ("Salary", &["Base Salary", "Overtime", "Commission"]),
    ("Bonus", &["Performance", "Holiday", "Referral"]),
    (
        "Freelance & Business",
        &["Client Payments", "Product Sales", "Service Income"],
    ),
    (
        "Investment Income",
        &["Dividends", "Interest", "Capital Gains", "Crypto Rewards"],
    ),
    ("Rental Income", &["Property Rent"]),
    (
        "Government Benefits",
        &[
            "Pension",
            "Unemployment",
            "Child Benefits",
            "Social Assistance",
        ],
    ),
    (
        "Refunds & Reimbursements",
        &[
            "Purchase Refund",
            "Tax Refund",
            "Insurance Reimbursement",
            "Expense Reimbursement",
        ],
    ),
    ("Gifts", &["Cash Gift", "Inheritance"]),
    ("Other Income", &["Miscellaneous"]),
];

const TRANSFER_CATEGORIES: CategoryTree = &[(
    "Transfers",
    &[
        "Checking ↔ Savings",
        "Investment Transfers",
        "Credit Card Payments",
        "Cash Withdrawal",
        "Cash Deposit",
        "Currency Exchange",
    ],
)];

/// Populates a freshly-created database with a curated set of top-level
/// categories and subcategories, so the user isn't staring at an empty
/// category picker on first run. Every name here is a fixed, known-valid
/// literal, so constructor failures are treated as a programmer error
/// (`expect`), not a runtime condition callers need to handle.
pub fn seed_default_categories(conn: &Connection) -> Result<(), RepositoryError> {
    let repo = SqliteCategoryRepository::new(conn);
    for tree in [EXPENSE_CATEGORIES, INCOME_CATEGORIES, TRANSFER_CATEGORIES] {
        for (parent_name, children) in tree {
            let parent = Category::new(
                CategoryId::new(),
                CategoryName::new(parent_name).expect("seed category name is valid"),
                None,
            )
            .expect("seed top-level category has no parent");
            repo.insert(&parent)?;
            for child_name in *children {
                let child = Category::new(
                    CategoryId::new(),
                    CategoryName::new(child_name).expect("seed category name is valid"),
                    Some(parent.id()),
                )
                .expect("seed child cannot be its own parent");
                repo.insert(&child)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("scrat.db");
        crate::create_new(&path, "test passphrase").unwrap()
    }

    #[test]
    fn seeding_happens_automatically_on_create_new() {
        let conn = test_conn();
        let repo = SqliteCategoryRepository::new(&conn);

        let all = repo.list_all().unwrap();

        assert_eq!(all.len(), 157);
    }

    #[test]
    fn seeded_subcategory_points_at_its_seeded_parent() {
        let conn = test_conn();
        let repo = SqliteCategoryRepository::new(&conn);
        let all = repo.list_all().unwrap();

        let groceries = all
            .iter()
            .find(|c| c.name().as_str() == "Groceries")
            .unwrap();
        let supermarket = all
            .iter()
            .find(|c| c.name().as_str() == "Supermarket")
            .unwrap();

        assert_eq!(groceries.parent_id(), None);
        assert_eq!(supermarket.parent_id(), Some(groceries.id()));
    }

    #[test]
    fn seeded_categories_respect_the_two_level_hierarchy() {
        let conn = test_conn();
        let repo = SqliteCategoryRepository::new(&conn);
        let all = repo.list_all().unwrap();

        for child_name in TRANSFER_CATEGORIES[0].1 {
            let child = all
                .iter()
                .find(|c| c.name().as_str() == *child_name)
                .unwrap();
            assert!(!scrat_domain::category::has_children(child.id(), &all));
        }
    }
}
