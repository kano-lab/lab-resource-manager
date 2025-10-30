# User Guide

This guide is for users of the deployed lab-resource-manager system.

## Slack Commands

To use the system, you first need to register your email address.

### Register Your Email Address

```
/register-calendar <your-email@example.com>
```

Register your email address and link it to your Slack account. This grants you access to all resource collections
(GPU servers, meeting rooms, etc.).

**Example:**
```
/register-calendar alice@example.com
```

### Register Another User (Administrators)

Administrators can register other users' email addresses:

```
/link-user <@slack_user> <email@example.com>
```

**Example:**
```
/link-user @bob bob@example.com
```

## Resource Reservation Syntax

### Device Specification Format

When reserving resources like GPU servers, you can specify which devices to use in the calendar event title.

#### Basic Specification Methods

- **Single Device**: `0` → Device 0
- **Range**: `0-2` → Devices 0, 1, 2
- **Multiple**: `0,2,5` → Devices 0, 2, 5
- **Mixed**: `0-1,6-7` → Devices 0, 1, 6, 7

#### Reservation Examples

In the calendar event title, write as follows:

```
[GPU 0-2] Machine Learning Model Training
```

This reserves devices 0, 1, and 2.

```
[GPU 0,3,5] Data Processing
```

This reserves devices 0, 3, and 5.

### Meeting Room Reservations

For meeting room reservations, device specification is not needed. Simply create a calendar event as usual.

```
Lab Meeting
```

## Notifications

### Reservation Start Notifications

When your reservation time arrives, a notification is sent to Slack. The notification includes:

- Reserving user's name (with mention if linked to an email address)
- Resource name (GPU server name, meeting room name, etc.)
- Devices in use (for GPUs)
- Reservation period

### Mention Feature

When you register your email address with the `/register-calendar` command, you will be automatically mentioned in
reservation notifications. This allows you to quickly confirm when your reservation starts.

## Frequently Asked Questions

### Q: Is email address registration required?

A: Not required, but registering provides these benefits:
- Automatic access to resource collections (like Google Calendar)
- Automatic mentions in reservation notifications

### Q: Can I register multiple email addresses?

A: Only one email address can be registered per Slack account.

### Q: Is device specification case-sensitive?

A: No, `[GPU 0-2]` and `[gpu 0-2]` are interpreted the same way.

### Q: How do I cancel a reservation?

A: Delete or cancel the event directly from the calendar. The system will automatically detect the change.
