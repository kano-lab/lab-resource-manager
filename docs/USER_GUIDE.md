# User Guide

## Table of Contents

- [Introduction](#introduction)
  - [Reservations and Reserved Slots](#reservations-and-reserved-slots)
- [Getting Started](#getting-started)
  - [1. Register Your Email Address](#1-register-your-email-address)
  - [2. Reserve a Resource](#2-reserve-a-resource)
- [Reserving Directly in Your Calendar App](#reserving-directly-in-your-calendar-app)
  - [Recurring Reservations](#recurring-reservations)
- [Notifications](#notifications)
  - [Notification Content](#notification-content)
  - [Buttons on Notification Messages](#buttons-on-notification-messages)
  - [Notifications for Recurring Reservations](#notifications-for-recurring-reservations)
  - [When a Reserved Slot Goes Unused](#when-a-reserved-slot-goes-unused)
- [Using AI Agents via MCP (Optional)](#using-ai-agents-via-mcp-optional)
- [Slack Command Reference](#slack-command-reference)
  - [`/register-calendar`](#register-calendar)
  - [`/reserve`](#reserve)
  - [`/free`](#free)
  - [`/mcp-token`](#mcp-token)

## Introduction

lab-resource-manager is a reservation system that manages laboratory resources, such as GPU servers and
meeting rooms, as events in a calendar app.

There are two ways to reserve a resource: use the `/reserve` command in Slack to open a modal, or
[open your calendar app directly and create an event](#reserving-directly-in-your-calendar-app). Both
produce the same kind of reservation, so feel free to stick with whichever one you like.

The full list of commands is in the [Slack Command Reference](#slack-command-reference) at the end.

### Reservations and Reserved Slots

This guide keeps these words apart:

| Word | Meaning |
| --- | --- |
| Reservation | A promise to hold resources for some time. It is what you create and cancel. It is either a one-time reservation or a recurring reservation |
| One-time reservation | A reservation for a single period |
| Recurring reservation | A reservation created in your calendar app with a repeat setting such as "weekly" |
| Reserved slot | A stretch of time a reservation actually holds. A one-time reservation has one; a recurring reservation has one each time it repeats |

For example, a recurring reservation for "every Monday, 10:00–12:00" holds the reserved slots
Nov 18 10:00–12:00, Nov 25 10:00–12:00, and so on. A reserved slot is time you have held with a
reservation, not an open slot waiting to be booked. Overlap checks, unused notices, and "End now"
work on reserved slots. For a one-time reservation, the reservation and its slot are the same thing.

## Getting Started

The first time you use this, two commands get you reserving:

### 1. Register Your Email Address

```text
/register-calendar <your-email@example.com>
```

This links your Slack user to the calendar app account that actually holds the reservations.
Register the email address you use with that calendar app.

**Registering grants you:**

- Automatic edit access to the calendar resources (GPU servers, meeting rooms, etc.)
- A Slack mention whenever you have a reservation

You'll get one invitation email from the calendar app for each resource you're granted access to, so a lab
with several GPU servers and rooms means several emails at once. Accept each one.

If you run `/reserve` before registering, the registration modal opens first, so you don't need to run
this command up front.

**Example:**

```text
/register-calendar alice@example.com
```

### 2. Reserve a Resource

```text
/reserve
```

This opens the reservation modal.

The modal asks for:

- **Resource type**: GPU server or meeting room
- **Server, or room**: chosen from a list depending on the type
- **Devices** (GPU servers only): checkboxes for which devices to use. Leave them all unchecked to reserve the whole server
- **Start and end time**: defaults to now through one hour from now
- **Notes** (optional): free text

On submit, you get an ephemeral message with the reservation ID if it succeeded, or one naming the
conflicting reservation, resource, and time if it didn't.

Use `/free` if you want to check what's available first. Its response also carries a "Reserve what's
free" button that opens this same modal.

**Note**: if the input is invalid, for example the start time is after the end time, the modal just
closes without an error message and no reservation is created. If your reservation doesn't show up,
double-check the times and try again.

## Reserving Directly in Your Calendar App

You can also reserve a resource without Slack, by opening your calendar app directly and creating an
event. The `/reserve` modal creates the same kind of event internally.

### Device Specification Format

When reserving resources like GPU servers, you can specify which devices to use in the calendar event title.

#### Basic Specification Methods

- **Single Device**: `0` → Device 0
- **Range**: `0-2` → Devices 0, 1, 2
- **Multiple**: `0,2,5` → Devices 0, 2, 5
- **Mixed**: `0-1,6-7` → Devices 0, 1, 6, 7

#### Reservation Examples

In the calendar event title, write the device specification:

```text
0-2
```

This indicates devices 0, 1, and 2 are in use.

```text
0,3,5
```

This indicates devices 0, 3, and 5 are in use.

**Note**: The event title should contain only the device specification. If you need to add a
description, use the event's description field instead.

### Meeting Room Reservations

For meeting room reservations, device specification is not needed. Simply create a calendar
event as usual.

```text
Lab Meeting
```

### Recurring Reservations

Creating an event with a repeat setting such as "weekly" makes a recurring reservation. Devices
are specified the same way as for a one-time reservation. Changing or deleting "this event only"
changes that reserved slot alone; the recurring reservation and its other slots stay as they are.

## Notifications

The system periodically checks the reservations in the calendar app and sends notifications to the
configured Slack channels when changes are detected.

### Notification Content

Notifications include the following information:

- User's name (with Slack mention if email address is registered)
- Resource name (GPU server name, meeting room name, etc.)
- Devices in use (for GPUs)
- Usage period

When you register your email address with the `/register-calendar` command, you will be automatically mentioned in Slack
for your reservations, making it easier to notice notifications.

### Buttons on Notification Messages

Notifications about a one-time reservation, and about a single reserved slot of a recurring
reservation, carry buttons for acting on that reserved slot.

- **🔄 更新 (Update)**: Change the time or the notes of the reserved slot
- **⏹️ 今で終了 (End now)**: End a running reserved slot at this moment and open the remaining
  time to others. The time you actually used stays on record
- **❌ 取り消す (Cancel)**: Drop the reserved slot entirely, leaving no record of the time used

Use "End now" when you finish earlier than planned, and "Cancel" when you are not going to use
the resource at all. On a slot of a recurring reservation, the button acts on that slot only;
the recurring reservation and its other slots stay. You can only act on your own reservations.

### Notifications for Recurring Reservations

A recurring reservation is announced as one reservation in a single message, not once per
reserved slot.

- **Creating a recurring reservation**: one "🔁 新規定期予約" message, with the interval
  (such as "毎週 月・水")
- **Cancelling a recurring reservation**: one "🗑️ 定期予約取り消し" message
- **A change reaching several reserved slots** (a new time of day, cutting it short):
  one "🔁 定期予約変更" message
- **Changing or ending early a single reserved slot**: one "🔄 定期予約の予約枠を変更" message
- **Cancelling a single reserved slot**: one "🗑️ 定期予約の予約枠を取り消し" message
- **A later slot coming into view as time passes**: nothing (no reservation was added)

Notifications about a recurring reservation as a whole carry no buttons: the buttons act on one
reserved slot, and a message about the whole reservation cannot say which.

### When a Reserved Slot Goes Unused

When GPU usage monitoring is enabled, the bot sends you a direct message when a
GPU in your reserved slot has gone unused for a while. There are two cases:

- **None of your processes are there**: nothing of yours is running on the GPU you reserved
- **Held without computing**: your processes are running and GPU memory is allocated, but no
  computation has run for a while — an inference server left resident, a notebook left open,
  a training job that stalled

The second kind of notice names which GPUs, how much memory you have allocated on them, and
the peak utilization seen over that stretch, so you can weigh it against what you know you
are doing. It waits longer than the first kind before reaching you.

GPUs are judged one at a time. Reserve eight and run one, and you hear about the other seven.
If you are still using the rest, leave things as they are; if you are done with all of them,
you can end the reservation and take out a smaller one.

- **⏹️ 今で終了する (End now)**: End the reserved slot at that moment and open the remaining time
  to others (the time you used stays on record)
- **✅ まだ使う (Still using it)**: You are about to use it — you won't be told about this slot for a while
- **❌ 予約を取り消す (Cancel the reservation)**: Cancel a one-time reservation. On a slot of a
  recurring reservation the button reads "❌ この予約枠だけ取り消す" (cancel this slot only), and
  the recurring reservation and its other slots stay

This is not meant to rush you. It exists so a reserved slot you held and forgot about doesn't
keep others waiting. If you still intend to use it, press "✅ Still using it" and carry on —
though the quiet lasts a while rather than forever. If it still goes unused after that, you
will hear about it again.

Whose processes they are does not matter: computation running inside Docker or any other
container still counts as your reserved slot being used.

Some reserved slots are never reported:

- Slots on a server that cannot be observed (without a report, whether it is in use is unknown)
- Meeting room slots
- Slots that are about to end

## Using AI Agents via MCP (Optional)

If the MCP (Model Context Protocol) server is enabled, you can let agents like
Claude Code view, create, change, end early, and cancel reservations directly on your
behalf, without going through Slack for every request.

### Get Your Access Token

```text
/mcp-token
```

This requires your email address to already be linked (via `/register-calendar`). The
token is shown only to you, in a message only you can see. Re-running the command issues a
new token and immediately revokes the previous one — keep your token private and don't
share it, since anyone holding it can act as you (create or cancel reservations in your
name).

### Configure Your MCP Client

Add the server to your client's MCP configuration (e.g. `.mcp.json`), using the URL your
admin provides and the token from `/mcp-token`:

```json
{
  "mcpServers": {
    "lab-resource-manager": {
      "url": "https://<lab-resource-manager host>:8787/mcp",
      "headers": {
        "Authorization": "Bearer <your token>"
      }
    }
  }
}
```

Ask your admin for the exact URL (host, port, and whether it's `http://` or `https://`).

### What You Can Do

- List the resources that can be reserved (GPU servers with their device numbers, meeting rooms)
- Check when each resource is free in a given period
- List all upcoming reserved slots, or just your own
- Look up a reserved slot, or a reservation (with its recurrence rule if it is recurring), by ID
- Create a one-time reservation (GPU server or meeting room)
- Change the time or notes of one of your reserved slots
- End one of your running reserved slots early, releasing the remaining time
- Cancel one of your reserved slots, or a whole reservation (every slot of a recurring one)

Ending early shortens the reserved slot to the current time, so the time you used stays on
record. Cancel instead when the slot should not have existed at all.

You can only change, end early, or cancel your own reservations — the same rule that applies
to `/reserve` in Slack.

## Slack Command Reference

| Command | What it does |
| --- | --- |
| [`/register-calendar <email>`](#register-calendar) | Register your email address |
| [`/reserve`](#reserve) | Reserve a resource |
| [`/free`](#free) | List what's free right now |
| [`/mcp-token`](#mcp-token) | Get an MCP access token |

### `/register-calendar`

```text
/register-calendar <your-email@example.com>
```

Links your Slack user to the calendar app account that holds the reservations. Registering grants
automatic edit access to the calendar resources and gets you a Slack mention on your own reservations.

See [Getting Started](#1-register-your-email-address) for the full walkthrough.

### `/reserve`

```text
/reserve
```

Opens the reservation modal. Specify the resource type (GPU server or meeting room), the target,
devices, start/end time, and notes, then submit to create the reservation.

See [Getting Started](#2-reserve-a-resource) for the full walkthrough.

### `/free`

```text
/free
```

Lists the resources you can use right now. The reply is visible only to you and is not posted to the channel.

### `/mcp-token`

```text
/mcp-token
```

Issues an access token that lets an AI agent act on reservations through the MCP server. Requires your
email address to already be linked via `/register-calendar`. The token is shown only to you; re-running
the command issues a new token and immediately revokes the old one. Don't share it.

See [Using AI Agents via MCP](#using-ai-agents-via-mcp-optional) for the setup walkthrough.
