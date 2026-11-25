# Modify Global Change

Builds or edits one global change: a set of conditions and the values to set on activities that meet them. Open it with **New...** or **Modify...** in the [Global Change](global-change) window.

## What is on it
- **Global Change Name**: the name shown in the list. It must not match another saved global change.
- **Select Subject Area**: what the change acts on. It is always **Activities**.
- **If**: the conditions. The first row is the heading: pick **(All of these)** when an activity must meet every condition, or **(Any of these)** when one is enough. Below it, each "Where" row has a **Parameter** (the field to test, such as WBS, Activity Name, an activity code or a user defined field), an **Is** test (such as **equals**, **contains** or **is greater than**), a **Value** and, for range tests, a **High Value**. For WBS and activity codes, **is under** matches the element or code value you type and everything below it; picking WBS starts on **is under**. A global change saved by an earlier version that joins its rows both ways keeps an **And** or **Or** at the end of each row until you pick a heading.
- **Then**: what to set on activities that meet the conditions. Each row reads **Parameter** = **Value**, and can add an **Operator** with a second **Value**: + adds, - subtracts, x multiplies, / divides and & joins text. A number added to a date counts calendar days. A value can be a number, text, a date or the name of another column; put text in double quotes to keep its spaces. The columns you can type in are listed as Then parameters, user defined fields included. The first Then row has no label; every row after it reads "And".
- **Else**: what to set on the activities that do not meet the conditions. Leave it empty to leave them alone.
- **OK**: checks and saves the global change, and goes back to the list.
- **Cancel**: closes without saving.
- **Change**: runs the change as it stands now and opens the **Global Change Report**, without saving the definition. This window stays open.
- **Add**: adds a row to the section you clicked last (If, Then or Else). Click the section name or one of its rows to pick it.
- **Delete**: removes the selected row after you confirm it with **Yes**, which Enter picks; **No** keeps it. It is available once a row is selected.
- **Move arrows** (under the buttons): the up and down arrows move the selected row one place within its section, and it stays selected. An arrow that cannot move the row (up on the first row, down on the last) is greyed. The left and right arrows are not used.

## Common tasks
To add two days to every piping activity's original duration:
1. Type a **Global Change Name**, such as "Piping plus 2 days".
2. In **If**, set **Parameter** to Activity Name, **Is** to **contains**, and **Value** to Pipe.
3. In the **Then** row, set **Parameter** to Original Duration, **Value** to Original Duration, **Operator** to +, and the second **Value** to 2d.
4. Click **OK**, then **Apply Change** in the list.

## Good to know
> OK stops and shows the problem when the name is empty or already used, a Then or Else row has no **Parameter**, or an **Operator** has no second **Value**. Values that cannot be used show up later as messages in the report.
> Nothing in the project changes until you click **Commit Changes** in the report.
> The same global changes run from the command line and from AI agents (`global-change`), on every activity of the project.
> If rows with no **Parameter** are ignored. With none set, the Then rows apply to every activity the Activities view's filter shows (every activity when it shows all of them).

## See also
- [Global Change](global-change)
- [Filter editor](filter-editor)
