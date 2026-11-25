# Activity Network Options

Sets how the Activity Network draws its boxes and arranges them. Choose `View > Activity Network > Activity Network Options...` while the network is shown on top, or right-click the network and choose **Activity Network Options...**.

## What is on it
- **Activity Box Template** tab: **Box template** picks what each box shows: **Standard**, **Dates** or **Activity ID and Name**.
- **Activity Network Layout** tab:
  - **Organization Options**: The network has one band per WBS element, and boxes that start on the same day stack in one column. **Organize along the top** starts each stack at the top of its band; **Organize centered** centers shorter stacks in the band.
  - **Network Order Options**: inside a stack, **Longest on Top** puts the activity with the most remaining duration first; **Shortest on Top** puts the least first.
  - **Horizontal spacing factor** and **Vertical spacing factor**: the gap between boxes, from 0.25 to 5 times normal.
  - **Show progress**: marks progress on boxes of started and finished activities.
  - **Always zoom to best fit after reorganize**: fits the whole network on screen after it is rearranged.
- **OK**: applies and closes. **Apply**: applies and stays open. **Cancel**: closes without changes. **Help**: opens this topic.
- **Font & Colors...**: opens **Activity Network Font and Colors**, with the box **Font size** (6 to 16) and the fill colors of **Normal** and **Critical** boxes. Its **Default** button puts the standard size and colors back.
- **Copy From...**: picks another saved layout under **Select the layout to copy from** and takes its box template and the places its boxes were dragged to. The list shows each layout's **Box Template**, and the **Moved** column shows how many of its boxes were dragged. Only saved layouts that hold Activity Network settings are listed.

## Common tasks
1. Show the network on top with `View > Show on Top > Activity Network`, then choose `View > Activity Network > Activity Network Options...`.
2. On **Activity Box Template**, choose **Activity ID and Name** for small boxes.
3. On **Activity Network Layout**, choose **Organize centered** and raise the **Vertical spacing factor** to 1.5.
4. Click **OK**, then use `View > Activity Network > Reorganize` to put every box back in its computed place.

## Good to know
> Boxes you drag by hand keep their place until you reorganize. `View > Activity Network > Reorganize` and **Reorganize All** on the right-click menu reset every box. **Reorganize** on the right-click menu, with boxes selected, resets only those boxes and the ones that follow them.
> These settings belong to the current layout. Save the layout to keep them.

## See also
- [Trace Logic Options](trace-logic-options)
- [Edit Relationship](edit-relationship)
- [Open Layout](open-layout)
