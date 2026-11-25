# Filter

Builds or changes one of your own filters: a set of rules that decides which activities show. Open it with **New...** or **Modify...** in the [Filters](filters) window.

## What is on it
- **Filter Name**: the name shown in the Filters list. It must be filled in and must not match another of your filters.
- **The rule grid**: one rule per row.
  - The first row says how the rules combine: **(All of the following)** means an activity must meet every rule, **(Any of the following)** means one is enough. Pick the other one from its list to switch.
  - **Parameter**: the column the rule tests, such as Activity Name, Start or Total Float.
  - **Is**: the test, such as **equals**, **is not equal to**, **contains**, **does not contain**, **starts with**, **is less than**, **is greater than**, **is within range of** or **is not within range of**.
  - **Value**: what to compare with. For a range, the low end.
  - **High Value**: the high end of a range test. It shows only for range tests.
- **Add**: adds a new rule row after the selected rule, or inside the selected group.
- **Delete**, **Cut**, **Copy** and **Paste**: act on the selected rule, together with any rules grouped under it.
- **Arrow pad**: up and down move the selected rule. Right puts it inside the group above it; left takes it back out.
- **OK**: checks the filter and hands it back to the Filters window. **Cancel** closes without changes.
- **Help**: opens this topic.

## Common tasks
1. In the Filters window, click **New...**.
2. Type a **Filter Name**, for example "Piping not started".
3. In the first rule, set **Parameter** to Activity Name, **Is** to **contains**, and **Value** to Pipe.
4. Click **Add** and set a second rule: Activity % Complete, **equals**, 0.
5. Click **OK**, tick the new filter in the Filters window, and click **OK** there to keep it.

## Nested groups
To mix "all" and "any", set a rule's **Parameter** to **(Any of the following)** or **(All of the following)**. That row starts a group. With the group row selected, **Add** puts a new rule inside it; for rules already below it, use the right arrow to move them in.

## Good to know
> Date values can be relative to a moving date. Use the drop button in the **Value** cell to pick Data Date, Current Date, Project Start, Project Finish, Start of This Week or Start of This Month, then type an offset after it if you want one, for example DD + 3w (d for days, w for weeks, m for months).
> A value that does not read as the parameter's type (a word in a date rule, say) stops OK and shows an error.

## See also
- [Filters](filters)
- [Columns](columns)
