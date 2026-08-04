//! Generates a fabricated demo database with a handful of accounts and about
//! six months of realistic-looking transactions, for recording product demo
//! videos. Not part of the app itself — run with:
//!
//!   cargo run -p scrat --example seed_sample_db
//!
//! Writes to `sample-data/scrat-demo.db` (gitignored, never committed) and
//! prints the passphrase needed to open it.

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use scrat_application::account_service::AccountService;
use scrat_application::transaction_service::{ImportRow, TransactionService};
use scrat_application::transfer_rule_service::TransferRuleService;
use scrat_domain::category::{Category, CategoryId};
use scrat_domain::money::Currency;
use scrat_domain::ports::CategoryRepository;
use scrat_domain::transaction::OperationKind;
use scrat_infra_sqlite::{
    create_new, set_currency_code, SqliteAccountRepository, SqliteCategoryRepository,
    SqliteTransactionRepository, SqliteTransferRuleRepository,
};
use std::path::PathBuf;

const PASSPHRASE: &str = "ScratDemo2026!";

/// Tiny deterministic xorshift PRNG — good enough for jittering demo amounts
/// and dates without pulling in a `rand` dependency for a one-off script.
struct Rng(u32);

impl Rng {
    fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }

    /// Inclusive range.
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        let span = (hi - lo + 1) as u32;
        lo + (self.next_u32() % span) as i64
    }

    fn chance(&mut self, percent: u32) -> bool {
        self.next_u32() % 100 < percent
    }

    fn pick<'a>(&mut self, options: &'a [&'a str]) -> &'a str {
        options[self.next_u32() as usize % options.len()]
    }
}

fn category_id(categories: &[Category], top: &str, sub: Option<&str>) -> CategoryId {
    let parent = categories
        .iter()
        .find(|c| c.parent_id().is_none() && c.name().as_str() == top)
        .unwrap_or_else(|| panic!("seed category tree is missing top-level '{top}'"));
    match sub {
        None => parent.id(),
        Some(sub_name) => categories
            .iter()
            .find(|c| c.parent_id() == Some(parent.id()) && c.name().as_str() == sub_name)
            .unwrap_or_else(|| panic!("seed category tree is missing '{top}' > '{sub_name}'"))
            .id(),
    }
}

fn main() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent directory")
        .join("sample-data")
        .join("scrat-demo.db");

    if path.exists() {
        std::fs::remove_file(&path).expect("remove previous demo database");
    }

    let conn = create_new(&path, PASSPHRASE).expect("create demo database");
    set_currency_code(&conn, "EUR").expect("set demo currency");
    let currency = Currency::new("EUR").expect("EUR is a valid currency code");

    let accounts_repo = SqliteAccountRepository::new(&conn, currency.clone());
    let categories_repo = SqliteCategoryRepository::new(&conn);
    let transactions_repo = SqliteTransactionRepository::new(&conn, currency.clone());
    let transfer_rules_repo = SqliteTransferRuleRepository::new(&conn);

    let account_service = AccountService::new(&accounts_repo, currency.clone());
    let transaction_service = TransactionService::new(
        &transactions_repo,
        &accounts_repo,
        &categories_repo,
        currency.clone(),
    );
    let transfer_rule_service = TransferRuleService::new(&transfer_rules_repo, &accounts_repo);

    let checking = account_service
        .create_account("Compte Courant")
        .expect("create checking account");
    let savings = account_service
        .create_account("Livret A")
        .expect("create savings account");
    let credit_card = account_service
        .create_account("Carte Visa")
        .expect("create credit card account");

    let categories = categories_repo.list_all().expect("list seeded categories");
    let cat = |top: &str, sub: Option<&str>| category_id(&categories, top, sub);

    let salary_cat = cat("Salary", Some("Base Salary"));
    let bonus_cat = cat("Bonus", Some("Performance"));
    let refund_cat = cat("Refunds & Reimbursements", Some("Purchase Refund"));
    let interest_cat = cat("Investment Income", Some("Interest"));
    let rent_cat = cat("Housing", Some("Rent"));
    let furniture_cat = cat("Housing", Some("Furniture"));
    let internet_cat = cat("Utilities", Some("Internet"));
    let mobile_cat = cat("Utilities", Some("Mobile Phone"));
    let electricity_cat = cat("Utilities", Some("Electricity"));
    let streaming_cat = cat("Utilities", Some("TV & Streaming"));
    let gym_cat = cat("Sports & Fitness", Some("Gym"));
    let home_insurance_cat = cat("Insurance", Some("Home"));
    let groceries_cat = cat("Groceries", None);
    let restaurant_cat = cat("Food & Drink", Some("Restaurant"));
    let bar_cat = cat("Food & Drink", Some("Bar"));
    let fuel_cat = cat("Transportation", Some("Fuel"));
    let transit_cat = cat("Transportation", Some("Public Transit"));
    let movies_cat = cat("Entertainment", Some("Movies"));
    let concerts_cat = cat("Entertainment", Some("Concerts"));
    let clothes_cat = cat("Clothing", Some("Clothes"));
    let doctor_cat = cat("Healthcare", Some("Doctor"));
    let pharmacy_cat = cat("Healthcare", Some("Pharmacy"));
    let haircuts_cat = cat("Personal Care", Some("Haircuts"));
    let gifts_cat = cat("Gifts & Donations", Some("Gifts"));
    let cash_withdrawal_cat = cat("Transfers", Some("Cash Withdrawal"));
    let investment_transfer_cat = cat("Transfers", Some("Investment Transfers"));

    let rule = transfer_rule_service
        .create_rule("epargne", savings.id())
        .expect("create checking-to-savings transfer rule");

    let today = NaiveDate::from_ymd_opt(2026, 8, 4).expect("valid date");
    let start = NaiveDate::from_ymd_opt(2026, 2, 1).expect("valid date");

    let mut rng = Rng(0x5EED_1234);
    let mut transfer_rows: Vec<ImportRow> = Vec::new();
    let mut last_fuel_fillup = start - Duration::days(10);
    let mut last_haircut = start - Duration::days(20);

    let mut date = start;
    while date <= today {
        let day = date.day();

        // --- Fixed monthly commitments on checking ---
        if day == 1 {
            transaction_service
                .create_transaction(date, -95_000, "Loyer Appartement", rent_cat, checking.id())
                .expect("create rent transaction");
        }
        if day == 2 {
            transaction_service
                .create_transaction(date, -75_00, "Navigo Mensuel", transit_cat, checking.id())
                .expect("create transit pass transaction");
        }
        if day == 3 {
            transaction_service
                .create_transaction(date, -29_99, "Basic-Fit Abonnement", gym_cat, checking.id())
                .expect("create gym transaction");
        }
        if day == 5 {
            transaction_service
                .create_transaction(
                    date,
                    -39_99,
                    "Orange Internet Fibre",
                    internet_cat,
                    checking.id(),
                )
                .expect("create internet transaction");
            transaction_service
                .create_transaction(
                    date,
                    -19_99,
                    "Free Mobile Forfait",
                    mobile_cat,
                    checking.id(),
                )
                .expect("create mobile transaction");
        }
        if day == 8 {
            transaction_service
                .create_transaction(date, -15_99, "Netflix", streaming_cat, checking.id())
                .expect("create streaming transaction");
        }
        if day == 10 {
            let amount = -rng.range(55_00, 85_00);
            transaction_service
                .create_transaction(
                    date,
                    amount,
                    "EDF Electricite",
                    electricity_cat,
                    checking.id(),
                )
                .expect("create electricity transaction");
        }
        if day == 12 {
            transaction_service
                .create_transaction(
                    date,
                    -18_50,
                    "MAIF Assurance Habitation",
                    home_insurance_cat,
                    checking.id(),
                )
                .expect("create home insurance transaction");
        }
        if day == 14 {
            transaction_service
                .create_transaction(
                    date,
                    -10_99,
                    "Spotify Premium",
                    streaming_cat,
                    checking.id(),
                )
                .expect("create spotify transaction");
        }
        if day == 25 {
            let amount = rng.range(6_00, 12_00);
            transaction_service
                .create_transaction(
                    date,
                    amount,
                    "Interets Livret A",
                    interest_cat,
                    savings.id(),
                )
                .expect("create interest transaction");
        }
        if day == 27 {
            transfer_rows.push(ImportRow {
                date,
                amount_minor_units: -300_000,
                description: "Virement vers Epargne".to_string(),
                category_id: investment_transfer_cat,
                operation_kind: OperationKind::BankTransfer,
            });
        }
        if day == 28 {
            transaction_service
                .create_transaction(
                    date,
                    280_000,
                    "Virement Salaire ACME SARL",
                    salary_cat,
                    checking.id(),
                )
                .expect("create salary transaction");
        }

        // --- One-off events ---
        if date.year() == 2026 && date.month() == 6 && day == 28 {
            transaction_service
                .create_transaction(date, 500_000, "Prime Performance", bonus_cat, checking.id())
                .expect("create bonus transaction");
        }
        if date.year() == 2026 && date.month() == 4 && day == 15 {
            transaction_service
                .create_transaction(
                    date,
                    45_000,
                    "Remboursement Zalando",
                    refund_cat,
                    checking.id(),
                )
                .expect("create refund transaction");
        }
        if date.year() == 2026 && date.month() == 3 && day == 20 {
            transaction_service
                .create_transaction(date, -32_000, "IKEA", furniture_cat, credit_card.id())
                .expect("create furniture transaction");
        }

        // --- Weekday-driven spending ---
        let weekday = date.weekday();
        if matches!(weekday, Weekday::Mon | Weekday::Thu) {
            let amount = -rng.range(25_00, 90_000);
            let merchant: &str = rng.pick(&["Carrefour City", "Monoprix", "Franprix", "Lidl"]);
            transaction_service
                .create_transaction(date, amount, merchant, groceries_cat, checking.id())
                .expect("create groceries transaction");
        }
        if matches!(weekday, Weekday::Wed | Weekday::Fri | Weekday::Sat) && rng.chance(55) {
            let amount = -rng.range(12_00, 60_00);
            let is_bar = matches!(weekday, Weekday::Fri | Weekday::Sat) && rng.chance(40);
            let (merchant, category) = if is_bar {
                (rng.pick(&["O'Sullivans Pub", "Le Comptoir"]), bar_cat)
            } else {
                (
                    rng.pick(&["Le Bistrot Parisien", "Cafe de Flore", "Sushi Wasabi"]),
                    restaurant_cat,
                )
            };
            transaction_service
                .create_transaction(date, amount, merchant, category, credit_card.id())
                .expect("create dining transaction");
        }
        if matches!(weekday, Weekday::Sat | Weekday::Sun) && rng.chance(25) {
            let amount = -rng.range(12_00, 45_00);
            let (merchant, category) = if rng.chance(50) {
                ("UGC Cine Cite", movies_cat)
            } else {
                ("Zenith Concert", concerts_cat)
            };
            transaction_service
                .create_transaction(date, amount, merchant, category, credit_card.id())
                .expect("create entertainment transaction");
        }
        if date >= last_fuel_fillup + Duration::days(14) && rng.chance(80) {
            let amount = -rng.range(55_00, 75_00);
            transaction_service
                .create_transaction(date, amount, "Total Energies", fuel_cat, checking.id())
                .expect("create fuel transaction");
            last_fuel_fillup = date;
        }
        if date >= last_haircut + Duration::days(42) && rng.chance(70) {
            transaction_service
                .create_transaction(
                    date,
                    -35_00,
                    "Jean Louis David",
                    haircuts_cat,
                    checking.id(),
                )
                .expect("create haircut transaction");
            last_haircut = date;
        }
        if weekday == Weekday::Sun && day <= 7 && rng.chance(60) {
            let amount = -rng.range(40_00, 12_000);
            let merchant: &str = rng.pick(&["Zara", "H&M", "Uniqlo"]);
            transaction_service
                .create_transaction(date, amount, merchant, clothes_cat, credit_card.id())
                .expect("create clothing transaction");
        }
        if day == 18 && rng.chance(50) {
            transaction_service
                .create_transaction(
                    date,
                    -80_00,
                    "Retrait DAB",
                    cash_withdrawal_cat,
                    checking.id(),
                )
                .expect("create cash withdrawal transaction");
        }
        if day == 22 && rng.chance(35) {
            transaction_service
                .create_transaction(
                    date,
                    -22_00,
                    "Pharmacie du Centre",
                    pharmacy_cat,
                    checking.id(),
                )
                .expect("create pharmacy transaction");
        }
        if day == 9 && rng.chance(20) {
            transaction_service
                .create_transaction(
                    date,
                    -25_00,
                    "Dr. Martin Generaliste",
                    doctor_cat,
                    checking.id(),
                )
                .expect("create doctor transaction");
        }
        if day == 6 && rng.chance(25) {
            let amount = -rng.range(25_00, 50_00);
            transaction_service
                .create_transaction(
                    date,
                    amount,
                    "Fleuriste du Marche",
                    gifts_cat,
                    credit_card.id(),
                )
                .expect("create gift transaction");
        }

        date += Duration::days(1);
    }

    transaction_service
        .import_transactions(&transfer_rows, checking.id(), &[rule])
        .expect("import monthly transfers to savings");

    account_service
        .establish_opening_balance(checking.id(), 320_000)
        .expect("establish checking opening balance");
    account_service
        .establish_opening_balance(savings.id(), 850_000)
        .expect("establish savings opening balance");
    account_service
        .establish_opening_balance(credit_card.id(), -45_000)
        .expect("establish credit card opening balance");

    let with_balances = account_service
        .list_accounts_with_balance()
        .expect("list accounts with balance");
    let total_transactions: usize = transaction_service
        .list_all()
        .expect("list all transactions")
        .len();

    println!("Demo database created at: {}", path.display());
    println!("Passphrase: {PASSPHRASE}");
    println!();
    for a in &with_balances {
        println!(
            "  {:<16} {:>10.2} EUR  ({} transactions)",
            a.account.name().as_str(),
            a.balance.minor_units() as f64 / 100.0,
            a.transaction_count
        );
    }
    println!("  Total transactions: {total_transactions}");
}
