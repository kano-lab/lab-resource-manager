# User Guide

This guide is for users of the deployed lab-resource-manager system.

## Slack Commands

### Register Your Email Address

```
/register-calendar <your-email@example.com>
```

This command links your Slack user with the default implementation (Google Calendar) and grants access to Google Calendar resources.
We recommend registering the Gmail address you regularly use with Google Calendar.

**Benefits of registration:**
- Automatically grants edit permissions to Google Calendar resources (GPU servers, meeting rooms, etc.)
- Enables Slack mentions in reservation notifications

**Example:**
```
/register-calendar alice@example.com
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

In the Google Calendar event title, write the device specification:

```
0-2
```

This indicates devices 0, 1, and 2 are in use.

```
0,3,5
```

This indicates devices 0, 3, and 5 are in use.

**Note**: The event title should contain only the device specification. If you need to add a description, use the event's description field instead.

### Meeting Room Reservations

For meeting room reservations, device specification is not needed. Simply create a Google Calendar event as usual.

```
Lab Meeting
```

## Notifications

The system periodically monitors Google Calendar resource usage and sends notifications to the configured Slack channels when changes are detected.

### Notification Content

Notifications include the following information:

- User's name (with Slack mention if email address is registered)
- Resource name (GPU server name, meeting room name, etc.)
- Devices in use (for GPUs)
- Usage period

When you register your email address with the `/register-calendar` command, you will be automatically mentioned in Slack
for your reservations, making it easier to notice notifications.
