# User Guide

## About This System

A system for reserving and managing lab resources such as GPU servers and rooms.

- **Reserve from Slack**
- **Reserve from Google Calendar**
- **Get notifications in Slack**

## How to Use

### 1. Initial Registration (First Time Only)

Run the `/reserve` command in Slack, and an email address input screen will appear.

Enter **the Gmail address you normally use with Google Calendar**.

- ✅ Access to Google Calendar is automatically granted
- ✅ Your Slack account is linked
- ✅ You will be mentioned in reservation notifications

### 2. Making Reservations (Two Methods)

#### Method 1: From Slack

```
/reserve
```

A modal will open where you can select the resource, time period, and devices, then submit.

#### Method 2: From Google Calendar

You can also reserve by directly creating events in Google Calendar.

**For GPU Server Reservations:**

Write the **device numbers you want to use** in the event title.

```
Examples:
0        → Use device 0 only
0-2      → Use devices 0,1,2
0,3,5    → Use devices 0,3,5
0-1,6-7  → Use devices 0,1,6,7
```

If you want to add notes, write them in the event description field.

**For Room Reservations:**

No need to specify device numbers. Just write a normal title.

```
Example: Lab Meeting
```

### 3. Updating or Canceling Reservations

- **From Slack**: Click the "🔄 Update" or "❌ Cancel" buttons on notification messages
- **From Google Calendar**: Directly edit or delete calendar events

Either way, changes will trigger notifications in Slack.

## FAQ

**Q: I don't know whose reservation this is?**
A: After completing initial registration with `/reserve`, you will be mentioned in your reservation notifications.

**Q: I can't see the reservation calendar in Google Calendar**
A: After completing initial registration with `/reserve`, access will be automatically granted and the calendar will appear.

**Q: The notifications are too noisy**
A: Mute the Slack notification channel.
