# Currencies

This dialog keeps the currencies used to price resources, with their exchange rates and how amounts are written. Open it with `Admin > Currencies...`. It also opens from a resource's currency picker, with a **Select** button added.

## What is on it
- **Currencies** list: **Currency ID**, **Currency Name** and **Exchange Rate**. The base currency is shown in bold: the currency the file names as its own, or else the one with a rate of 1.
- **General** tab, for the selected currency:
- **Currency ID**: a short code, up to 10 characters, unique.
- **Currency Name**: the full name.
- **Currency Symbol**: up to 5 characters, such as $.
- **Exchange Rate**: kept with the currency and written to the XER. It must be above zero, and it is locked at 1 for the base currency. OpenXER does not convert any amount with it: prices and costs show the numbers as stored.
- **Appearance** tab:
- **Decimal Symbol** and **Digit Grouping Symbol**: one character each, not a digit.
- **Number of Decimal Places**: 0 to 4.
- **Positive Currency Format** and **Negative Currency Format**: where the symbol and minus sign go. In the patterns, # is the symbol and 1.1 the number.
- **Example**: shows how an amount will look.
- **Close**, **Select** (only when opened from a picker), **Add**, **Delete** and **Help**.

## Common tasks
1. Click **Add**. A new currency gets an ID like C1, the $ symbol and a rate of 1.
2. On the General tab set the ID, name, symbol and **Exchange Rate**.
3. On the Appearance tab set the symbols and formats, and check the **Example**.

## Good to know
> Each field is saved when you leave it. A value that fails a check shows a message and is not saved.
> The base currency cannot be deleted. Deleting another currency moves the resources priced in it to the base currency, after you confirm.

## See also
- [Resources view](view-resources)
- [Admin Preferences](admin-preferences)
