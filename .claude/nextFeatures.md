# Features

## import CSV

### Category detection

Here we break the rules we set, I want an optionnal, default false, feature to search with a LLM the description on the web and match it to an existing category. It must not create a new category. It must use websites such as societe.com (i don't know about the other countries ones).
The llm is used through Ollama, it is on the machine, installed by the user.
The LLM only has access to description and existing categories.

In the settings we can test the connexion with the llm.

We do not add libraries to the app.

I am unsure about the searching internet part for the source. Review the feature request, help me figure out missing parts.

## License.md

Add a license file. The app is open source, can be forked whatever. Cannot be used for business and must be cited if used.

## Readme

Generate a README.md appropriate to the app, mention at the top the use of Claude to generate the code.

Add badges for this project.

## Transactions page - CSV import

### Settings

- Remove text: "Category set if none found"
- Find better text for options "Use previous transactions' categories to detect new transactions' categories" and "Prefer a category already used for this description over the CSV's own category"
- When I click on the default category selector, the search bar has a z index bellow the checkboxes of the window

### duplicates

On csv import, if a transaction has the same fields than an existing one (date, amount, description), we want to uncheck it by default and let the user know why the row was unticked. 

