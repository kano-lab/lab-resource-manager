# User Guide

This guide is for users of the deployed lab-resource-manager system.

## Slack Commands

### Register Your Email Address

```text
/register-calendar <your-email@example.com>
```

This command links your Slack user with the default implementation (Google Calendar) and grants
access to Google Calendar resources. We recommend registering the Gmail address you regularly use
with Google Calendar.

**Benefits of registration:**

- Automatically grants edit permissions to Google Calendar resources (GPU servers, meeting rooms, etc.)
- Enables Slack mentions in reservation notifications

**Example:**

```text
/register-calendar alice@example.com
```

### Check What Is Free Right Now

```text
/free
```

Lists the resources you can use right now. The reply is visible only to you and is not posted to the channel.

**Reading the output:**

```text
いま GPU 5台 が空いています

GPU             0  1  2  3
gpu-server-1    .  #  #  .
gpu-server-2    #  .  .  .

`.` 空き 　`#` 使用中

部屋
🔴 Meeting Room A — @tanaka 〜16:00

使用中のGPU
🔴 gpu-server-1 1,2 — @sato 〜18:00
🔴 gpu-server-2 0 — @tanaka 〜明日 09:00
```

Each column of the grid corresponds to a device number, showing which numbers are free.
The list below it names the reserver and end time for whatever is taken.

When everything is taken, the reply names the resource that frees up soonest and when.

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

For meeting room reservations, device specification is not needed. Simply create a Google Calendar
event as usual.

```text
Lab Meeting
```

## Notifications

The system periodically monitors Google Calendar resource usage and sends notifications to the
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

## When a Reservation Goes Unused

In labs where GPU usage monitoring is enabled, the bot sends you a direct message when
none of your processes have been running on a GPU you reserved for a while.

- **⏹️ End now**: End the reservation at that moment and open the remaining time to others
  (the time you used stays on record)
- **✅ Still using it**: You are about to use it — you won't be told about this reservation again
- **❌ Cancel**: Drop the reservation entirely

This is not meant to rush you. It exists so a reservation you held and forgot about doesn't
keep others waiting. If you still intend to use it, press "✅ Still using it" and carry on.

Some reservations are never reported:

- Reservations on a server that cannot be observed (without a report, whether it is in use
  is unknown)
- Reservations whose owner has no OS username linked via `/link-user` (your processes cannot
  be told apart from anyone else's)
- Meeting room reservations
- Reservations that are about to end

## Using AI Agents via MCP (Optional)

If your lab has enabled the MCP (Model Context Protocol) server, you can let agents like
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
