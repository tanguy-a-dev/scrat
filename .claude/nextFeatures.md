# Features

## Transactions page

## Transactions selection
On the left of the Expenses and Income lists there will be a selection checkbox. 
The checkbox will be invisible by default and appear when hovered.
Clicking on one checkbox on Expense list will make a global Expense checkbox appear.
Clicking on one checkbox on Income list will make a global Income checkbox appear.
With it you can select/unselect every transaction currently loaded.
Selecting a transaction will make a new menu appear on the right of Expense or Income depending on which is selected.
The menu will have the following features:
- delete icon - delete selected transactions
- pen icon - rename categories for selected transactions
The menu disappears if no items are selected.

### Set dates
Set by months first then by specific dates

## Loading transactions

- Transaction count on the top of the page, display count not currently loaded. If "Year" then the number is the count of transactions for the year, same for the other. It changes with filters like search source or category.
- icon arrow up button to go back up the page -> icon on the right instead of left

## Page Cache

If user is viewing Details by Year and goes to Category then back to Details, the page should still be on details by year filter.
If user scrolls thourgh transaction and filters then changes pages and comes back, user should be back where he left at

