# Notebooks and Project Codes

A notebook keeps notes under topics, such as scope, access or a change log. Activities, WBS elements and the project each have their own: the **Notebook** tab of Activity Details, WBS Details and Project Details. The topics come from the Notebook Topics dictionary; each topic says whether activities, WBS elements or projects can use it. The **Codes** tab of Project Details holds the project's code values.

## What is on the Notebook tab
- **Notebook Topic** list: the topics this activity, element or project has. Click one to show its text at the right.
- **Add**: opens **Assign Notebook Topic** with the topics not used yet. Select one or more and click **Assign**, or double-click one.
- **Delete**: takes the selected topic and its text away.
- The text pane, captioned with the topic's name, shows the text with its formatting: bold, italic, underlined, bulleted and numbered lines.
- **Modify**: opens the text for editing in place, with a formatting toolbar over it. **Print** is off: notes print with the layout. **Copy** puts the text on the clipboard without formatting.

## The formatting toolbar
- **Bold**, **Italic**, **Underline**: switch the format of the selected text. With nothing selected, the text you type next takes the change.
- **Bullets**, **Numbering**: make the lines of the selection a bulleted or a numbered list, or plain lines again when they already are one.
- **Clear Formatting**: makes the selected text plain and its lines ordinary lines.
- The Steps tab's step description uses the same pane and toolbar.

## Project codes
- The **Codes** tab lists each **Project Code** the project has, with its **Code Value** and **Code Description**.
- **Assign**: opens **Assign Project Codes**. Pick the code in the **Project Code** drop-down, select a value and click **Assign**. A project holds one value of each code; assigning another value of the same code replaces it.
- **Remove**: takes the selected value off the project after you confirm. The same button in **Assign Project Codes** removes the value selected there when the project has it.
- **New...** in **Assign Project Codes** adds a value to the code shown through **Add Code Value**; it is not assigned until you click **Assign**.

## Common tasks
1. Open the WBS view and select an element, then the **Notebook** tab of WBS Details.
2. Click **Add**, select a topic and click **Assign**.
3. Click **Modify**, type the note, select a word and click **Bold**.

## Good to know
> Each Add, Delete, Assign, Remove and formatting command is one step of `Edit > Undo`; typing in one go is one step too.
> Notes are kept in the file as HTML, so formatting written by other scheduling programs shows here and is kept when you edit. A note that holds pictures, tables or links can be read but not edited here, so nothing in it is lost.
> Agents set a WBS or project notebook with `set_wbs_notebook` and a project code with `set_project_code`.

## See also
- [Assign Resources, Roles and Activity Codes](assign-dialogs)
- [Codes](codes)
- [WBS view](view-wbs)
- [Projects view](view-projects)
