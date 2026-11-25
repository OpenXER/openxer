# Assign Resources, Roles and Activity Codes

The windows that add resources, roles, activity codes, notebook topics, documents and risks to the selected activity all work the same way. Open them from `Edit > Assign`, the activity table's right-click menu, or the **Add** and **Assign** buttons on the tabs of the activity details. Each one stays open while you work in the table.

## What is on them
- **The list**: the things you can assign. Click one to select it. Hold `Ctrl` and click to add another one to the selection, or to take one out. Hold `Shift` and click to select every line from the last one you clicked to this one.
- **Close** (the first button): closes the window. **Help** (the last): opens this topic.
- **Assign**: assigns every selected line at once. A double-click assigns just the line clicked. In Assign Resources it is off while the one line selected is a resource the activity already has.
- **Remove**: with one line selected that the activity already has, takes it off the selected activities. It asks first, as **Remove** on the activity details does.
- **Replace** (Assign Resources): with one of the activity's resources selected, click **Replace**, select the resource to put in its place and click **Assign**. The assignment stays; its price and units per time follow your User Preferences, as when you pick another resource on the **Resources** tab.
- **New...** (Assign Activity Codes): once a value is selected, opens **Add Code Value** for a new value of the code shown. Type the **Code Value** and, if you like, a **Code Value Description**, then click **OK**. The new value is added to the code's dictionary and selected in the list; it is not assigned until you click **Assign**.
- **New** (Assign Resources): adds a resource by its ID and name without leaving the window.
- **Assign Project Codes** (opened from the Codes tab of Project Details): the same buttons, for the open project. **Remove** takes the selected value off the project, after you confirm. The WBS and Project Details Notebook tabs open **Assign Notebook Topic** for the element or the project; see [Notebooks and Project Codes](notebooks).

Assign Resources opens with the resource of the assignment selected on the **Resources** tab highlighted.

## Common tasks
1. Select the activity in the activity table.
2. Choose `Edit > Assign > Resources...`.
3. Click the first resource, then `Ctrl`+click the others the activity needs.
4. Click **Assign**. Each selected resource becomes an assignment of the activity.

## Good to know
> An activity holds one value of each activity code. When several values of one code are selected, **Assign** gives the activity the first of them.
> With several activities selected in the table, the window says "Assigns to the N selected activities." Each one gets everything selected in the list, as one undo step.
> A resource already assigned to an activity in the same role is skipped when it is assigned to several activities at once.
> Milestones take no resources or roles.

## See also
- [Assign Predecessors and Successors](assign-relationships)
- [Codes](codes)
- [Activities view](view-activities)
