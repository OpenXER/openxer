# Documents

The Documents view lists the open project's work products and documents: drawings, specifications, procedures, reports and anything else the work produces or depends on. Open it with `Project > Documents` or the **Documents** button on the Project bar.

## What it shows
- **Layout: Work Products and Documents**: the strip above the table. Click it to **Expand All**, **Collapse All**, or show or hide the details pane with **Details**.
- **Document table**: an outline of documents, each under its parent document, with **Title**, **Reference No.**, **Deliverable**, **Status**, **Document Category**, **Author**, **Version**, **Date**, **Private Location** and **Public Location**.
- **Command bar**: **Add** and **Delete** on the right edge.
- **WP & Doc Details**: tabs for the selected document.
- **General**: title, reference, version, author, date, status, category and the **Deliverable** check box.
- **Description**: free text about the document.
- **Files**: the private and public locations, typed as file paths. The **Launch...** button beside each opens that file; it is greyed out while the location is empty.
- **Assignments**: the activities and WBS elements the document is tied to, with **Assign Activity**, **Assign WBS**, **Remove** and **Work Product** (marks the assignment as a work product: something the activity delivers).
- **User Defined Fields**.

## Common tasks
1. Select a document at the level where the new one belongs and click **Add** (or press `Insert`). The new document is added last at that level.
2. Fill in the **General** tab.
3. On **Assignments**, click **Assign Activity** or **Assign WBS** and pick where the document applies.
4. To open the file, click **Launch...** beside its location on the **Files** tab, or right-click the row and choose **Launch** and then **Private Document** or **Public Document**.

Right-click a row for **Add**, **Delete**, **Assign Activity**, **Assign WBS**, **Remove** (takes off the assignment selected on the **Assignments** tab), **Launch**, **WP & Doc Details** and **Hide Detail Window**.

## Good to know
> **Customize Columns...** on the right-click menu picks and orders the columns; the choice is kept with the view layout.
> **Delete** removes the document, every document under it, and their assignments.
> Only a file on a local drive (such as `C:\...`) that exists and is a document, drawing, picture or mail file is opened. Programs and scripts (such as `.exe`, `.bat`, `.cmd`, `.ps1` or `.js` files), archives, web addresses and network paths are not. Before it opens the file, OpenXER shows the full path and asks you to confirm, because the location comes from the project file.
> The XER file stores only the locations, not the files themselves.
> AI agents can read this list and the text of these files through OpenXER's
  tool server, with the same checks as **Launch...**: text, Word (.docx) and
  PDF files on a local drive only. See [Working with AI agents](working-with-ai-agents).

## See also
- [Activities](view-activities)
- [WBS](view-wbs)
- [Categories and Lists](categories-and-lists)
