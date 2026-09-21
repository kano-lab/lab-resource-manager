# User Guide

## Table of Contents

- [Introduction](#introduction)
- [Getting Started](#getting-started)
  - [1. Register Your Email Address](#1-register-your-email-address)
  - [2. Reserve a Resource](#2-reserve-a-resource)
- [Reserving Directly in Your Calendar App](#reserving-directly-in-your-calendar-app)
- [Notifications](#notifications)
  - [Notification Content](#notification-content)
  - [Buttons on Notification Messages](#buttons-on-notification-messages)
  - [When a Reservation Goes Unused](#when-a-reservation-goes-unused)
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

## Notifications

The system periodically monitors resource usage in the calendar app and sends notifications to the
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

Reservation notifications carry buttons for acting on your own reservations.

- **🔄 Update**: Change the time or the notes of the reservation
- **⏹️ End now**: End a running reservation at this moment and open the remaining time to
  others. The time you actually used stays on record
- **❌ Cancel**: Drop the reservation entirely, leaving no record of the time used

Use "⏹️ End now" when you finish earlier than planned, and "❌ Cancel" when you are not
going to use the resource at all. You can only act on reservations you own.

### When a Reservation Goes Unused

When GPU usage monitoring is enabled, the bot sends you a direct message when a
GPU you reserved has gone unused for a while. There are two cases:

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

- **⏹️ End now**: End the reservation at that moment and open the remaining time to others
  (the time you used stays on record)
- **✅ Still using it**: You are about to use it — you won't be told about this reservation for a while
- **❌ Cancel**: Drop the reservation entirely

This is not meant to rush you. It exists so a reservation you held and forgot about doesn't
keep others waiting. If you still intend to use it, press "✅ Still using it" and carry on —
though the quiet lasts a while rather than forever. If it still goes unused after that, you
will hear about it again.

Whose processes they are does not matter: computation running inside Docker or any other
container still counts as your reservation being used.

Some reservations are never reported:

- Reservations on a server that cannot be observed (without a report, whether it is in use
  is unknown)
- Meeting room reservations
- Reservations that are about to end

## Using AI Agents via MCP (Optional)

If the MCP (Model Context Protocol) server is enabled, you can let agents like
Claude Code view, create, update, end early, and cancel reservations directly on your
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

- List all upcoming reservations, or just your own
- Look up a reservation by ID
- Create a new reservation (GPU server or meeting room)
- Update the time or notes on a reservation you own
- End a running reservation of yours early, releasing the remaining time
- Cancel a reservation you own

Ending early shortens the reservation to the current time, so the time you used stays on
record. Cancel instead when the reservation should not have existed at all.

You can only update, end early, or cancel reservations you own yourself — the same rule
that applies to `/reserve` in Slack.

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
