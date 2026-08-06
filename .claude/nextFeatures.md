# Features

## Details: Compare vue

## Transaction: reset filters next to Amount

## Details -> go to

## Transactions -> New Transaction (Icon Plus) 
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
We should either not export them or let the user chose by adding 1. "do not export transfers/mirrored transactions" on export csv 2. "do not import mirrored transactions" on import.
What do you think?

## Test Coverage

App coverage is rather low. generate and check the report. Check where to add relevant tests for the app and implement them.
