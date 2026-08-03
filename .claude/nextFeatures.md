# Features

## Transactions page

like date, amount, descriptions, category. I want Type and Account to be clickable to sort transactions.


## import CSV

### File size

Add safety: check for file size and reject if too big for a csv.

### Edit columns

- Amount layout: this option is not needed for the user since we can select Debit and Credit columns
- After the columns were guessed properly, if the user changes an amount column (debit/credit) the error message is 
"Only 100% of dates and 32% of amounts could be read — the columns were probably guessed wrong."
this makes no sense if the user sets this so maybe replace by something like "... - the columns seem wrongly set"

### Settings

Like we have a "Edit Columns" I do want a "Categories settings" spoiler panel.
We'll add "Prefer a category already used for this description over the CSV's own category".
We'll also add the option to use previous transactions categories to detect new transactions categories, which defaults to true.
We'll also move the Default category here, adding a description such as "Category set if none found"

### Category detection

Here we break the rules we set, I want an optionnal, default false, feature to search with a LLM the description on the web and match it to an existing category. It must not create a new category. It must use websites such as societe.com (i don't know about the other countries ones).
The llm is used through Ollama, it is on the machine, installed by the user.
The LLM only has access to description and existing categories.

In the settings we can test the connexion with the llm.

We do not add libraries to the app.

I am unsure about the searching internet part for the source. Review the feature request, help me figure out missing parts.

## Ops

## CI
Add a github CI which runs tests, coverage, linters

## CI - Security

I wish for a way to test test the app's security. I only know of Snyk but it's paid for, i want open source. RustSec maybe?

I should be able to test with a make command. If there's a package it must not be included for the prod app. 

## License.md

Add a license file. The app is open source, can be forked whatever. Cannot be used for business and must be cited if used.

## Readme

Generate a README.md appropriate to the app, mention at the top the use of Claude to generate the code.

Add badges for this project.

