# Features

## CSV import stripping

Transactions description with dates  

## Rust version

We are using rust edition = "2021", why not 2024?

## CI

### Coverage job:
"Annotations
4 warnings
security
Node.js 20 is deprecated. The following actions target Node.js 20 but are being forced to run on Node.js 24: actions/checkout@v4, actions/setup-node@v4. For more information see: https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/"

### Coverage

App coverage is rather low. generate and check the report. Check where to add relevant tests for the app and implement them.

### Build

Don't build an universal version for mac, it will enlarge the app for no reason, instead build for both architectures.

Also .rpm does not have doc contrary to the others

## Details: Compare vue

## Transaction: reset filters next to Amount

## Details -> go to

## Transactions -> Plus 
- Category search: reuse the one when searching category in transactions list
- Account search: Use the same as in transactions list, search for account
- Date picker: Must use the theme adapted to the app, as with "Set Dates" instead of black/white

## Transactions -> Sort
When I sort with filters: Date, Amount, Description, Type, Category, Account:
It only sorts the loaded transactions. If the user clicks on sort by date then he must have the most recent or oldest ones amongs all transactions, same goes for the other sorting methods.

## Transactions -> Import CSV

When importing a CSV, do not load all rows at once in the app, load 20 by 20 if the user scrolls down.

## Export transfers bug

If the user has account transfer rules, exports to .csv the transactions, and import them back, there will be duplicates.
