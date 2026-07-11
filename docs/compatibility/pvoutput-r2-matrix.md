# PVOutput r2 compatibility inventory

- Source: [https://pvoutput.org/help/api_specification.html](https://pvoutput.org/help/api_specification.html)
- Retrieved: 2026-07-11
- Services: 21
- Policy: documented donation-only limits become administrator-configurable; wire behavior remains represented.

| Service                         | Route                                    | Methods    | Parameter rows | Errors | Restrictions | Donation notes |
| ------------------------------- | ---------------------------------------- | ---------- | -------------: | -----: | -----------: | -------------: |
| Add Output Service              | `/service/r2/addoutput.jsp`              | GET / POST |             36 |      9 |            7 |              1 |
| Add Status Service              | `/service/r2/addstatus.jsp`              | GET / POST |             23 |     11 |            8 |              1 |
| Add Batch Status Service        | `/service/r2/addbatchstatus.jsp`         | GET / POST |              3 |      0 |            6 |              1 |
| Get Status Service              | `/service/r2/getstatus.jsp`              | GET        |             10 |      1 |            7 |              0 |
| Get Statistic Service           | `/service/r2/getstatistic.jsp`           | GET        |              5 |      0 |            2 |              0 |
| Get System Service              | `/service/r2/getsystem.jsp`              | GET        |              9 |      0 |            7 |              0 |
| Post System Service             | `/service/r2/postsystem.jsp`             | POST       |              6 |      0 |            2 |              0 |
| Get Ladder Service              | `/service/r2/getladder.jsp`              | GET        |              1 |      0 |            2 |              0 |
| Get Output Service              | `/service/r2/getoutput.jsp`              | GET        |              8 |      0 |            4 |              0 |
| Get Extended Service            | `/service/r2/getextended.jsp`            | GET        |              3 |      0 |            3 |              0 |
| Get Favourite Service           | `/service/r2/getfavourite.jsp`           | GET        |              1 |      0 |            5 |              0 |
| Get Missing Service             | `/service/r2/getmissing.jsp`             | GET        |              2 |      0 |            2 |              0 |
| Get Insolation Service          | `/service/r2/getinsolation.jsp`          | GET        |              4 |      0 |            2 |              0 |
| Delete Status Service           | `/service/r2/deletestatus.jsp`           | GET / POST |              2 |      6 |            1 |              0 |
| Search Service                  | `/service/r2/search.jsp`                 | GET / POST |              5 |      0 |            4 |              0 |
| Get Team Service                | `/service/r2/getteam.jsp`                | GET        |              1 |      1 |            1 |              0 |
| Join Team Service               | `/service/r2/jointeam.jsp`               | GET        |              1 |      4 |            2 |              0 |
| Leave Team Service              | `/service/r2/leaveteam.jsp`              | GET        |              1 |      3 |            1 |              0 |
| Get Supply Service              | `/service/r2/getsupply.jsp`              | GET        |              2 |      0 |            4 |              0 |
| Register Notification Service   | `/service/r2/registernotification.jsp`   | GET        |              3 |      0 |            5 |              1 |
| Deregister Notification Service | `/service/r2/deregisternotification.jsp` | GET        |              2 |      0 |            0 |              0 |

The machine-readable inventory preserves every documented section, table row, list item,
success/error heading, restriction, and donation note for conformance-test traceability.
