#!/usr/bin/env python3
"""Generate the dated PVOutput r2 contract inventory from the official specification."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from urllib.request import Request, urlopen

from bs4 import BeautifulSoup, Tag

SOURCE_URL = "https://pvoutput.org/help/api_specification.html"
EXPECTED_SERVICES = 21


def text(node: Tag) -> str:
    return " ".join(node.get_text(" ", strip=True).replace("\uf0c1", "").split())


def table_data(table: Tag) -> dict[str, object]:
    rows = []
    for row in table.select("tr"):
        cells = [text(cell) for cell in row.find_all(["th", "td"], recursive=False)]
        if cells:
            rows.append(cells)
    headers = rows[0] if rows else []
    return {"headers": headers, "rows": rows[1:]}


def section_data(section: Tag) -> dict[str, object]:
    heading = section.find(["h3", "h4"], recursive=False)
    title = text(heading) if heading else "Overview"
    belongs_here = lambda item: item.find_parent("section") is section
    paragraphs = [
        text(item)
        for item in section.find_all("p")
        if belongs_here(item) and text(item)
    ]
    lists = [
        [text(item) for item in listing.find_all("li", recursive=False)]
        for listing in section.find_all(["ul", "ol"])
        if belongs_here(listing)
    ]
    tables = [
        table_data(table)
        for table in section.find_all("table")
        if belongs_here(table)
    ]
    children = [
        section_data(child)
        for child in section.find_all("section", recursive=False)
    ]
    return {
        "title": title,
        "paragraphs": paragraphs,
        "lists": lists,
        "tables": tables,
        "subsections": children,
    }


def flatten(sections: list[dict[str, object]]) -> list[dict[str, object]]:
    result = []
    for section in sections:
        result.append(section)
        result.extend(flatten(section["subsections"]))
    return result


def documented_methods(service_text: str, name: str) -> list[str]:
    match = re.search(
        r"accepts?\s+(?:(?:both|the)\s+)?(POST\s+or\s+GET|GET\s+or\s+POST|GET|POST)\s+requests?",
        service_text,
        re.IGNORECASE,
    )
    if match:
        methods = re.findall(r"GET|POST", match.group(1).upper())
        return sorted(set(methods))
    if name.startswith("Post "):
        return ["POST"]
    if name.startswith(("Add ", "Delete ")):
        return ["GET", "POST"]
    return ["GET"]


def extract_services(soup: BeautifulSoup) -> list[dict[str, object]]:
    services = []
    for heading in soup.select("h2"):
        name = text(heading)
        if not name.endswith(" Service"):
            continue
        container = heading.parent
        if not isinstance(container, Tag):
            continue
        service_text = text(container)
        route_match = re.search(r"https://pvoutput\.org/service/r2/([a-z]+\.jsp)", service_text)
        if not route_match:
            raise RuntimeError(f"missing route for {name}")
        sections = [
            section_data(section)
            for section in container.find_all("section", recursive=False)
        ]
        all_sections = flatten(sections)
        parameter_tables = [
            table
            for section in all_sections
            if "Parameter" in section["title"]
            for table in section["tables"]
        ]
        success_sections = [
            section for section in all_sections if "Success" in section["title"]
        ]
        errors = [
            {
                "title": section["title"],
                "details": section["paragraphs"] + sum(section["lists"], []),
            }
            for section in all_sections
            if re.match(
                r"(?:Bad request|Bad Request|Unauthorized|Forbidden|Method Not Allowed|OK)\s+\d+:",
                section["title"],
            )
            and not section["title"].startswith("OK ")
        ]
        restrictions = [
            item
            for section in all_sections
            if "Restrictions and Limitations" in section["title"]
            for listing in section["lists"]
            for item in listing
        ]
        donation_features = [
            item
            for section in all_sections
            if "Donation Features" in section["title"]
            for listing in section["lists"]
            for item in listing
        ]
        services.append(
            {
                "name": name,
                "route": f"/service/r2/{route_match.group(1)}",
                "methods": documented_methods(service_text, name),
                "parameterTables": parameter_tables,
                "successSections": success_sections,
                "errors": errors,
                "restrictions": restrictions,
                "donationFeatures": donation_features,
                "sections": sections,
            }
        )
    if len(services) != EXPECTED_SERVICES:
        raise RuntimeError(f"expected {EXPECTED_SERVICES} services, found {len(services)}")
    return services


def common_contract(soup: BeautifulSoup) -> dict[str, object]:
    result = {}
    for heading in soup.select("h2"):
        name = text(heading)
        if name not in {"Getting Started", "Rate Limits", "HTTP Headers", "Common Errors"}:
            continue
        container = heading.parent
        if isinstance(container, Tag):
            sections = [
                section_data(section)
                for section in container.find_all("section", recursive=False)
            ]
            result[name] = {
                "overview": [text(item) for item in container.find_all("p", recursive=False)],
                "lists": [
                    [text(item) for item in listing.find_all("li", recursive=False)]
                    for listing in container.find_all(["ul", "ol"], recursive=False)
                ],
                "sections": sections,
            }
    return result


def markdown(inventory: dict[str, object]) -> str:
    lines = [
        "# PVOutput r2 compatibility inventory",
        "",
        f"- Source: [{SOURCE_URL}]({SOURCE_URL})",
        f"- Retrieved: {inventory['retrievedAt']}",
        f"- Services: {len(inventory['services'])}",
        "- Policy: documented donation-only limits become administrator-configurable; wire behavior remains represented.",
        "",
        "| Service | Route | Methods | Parameter rows | Errors | Restrictions | Donation notes |",
        "| --- | --- | --- | ---: | ---: | ---: | ---: |",
    ]
    for service in inventory["services"]:
        parameter_count = sum(len(table["rows"]) for table in service["parameterTables"])
        lines.append(
            "| {name} | `{route}` | {methods} | {parameters} | {errors} | {restrictions} | {donation} |".format(
                name=service["name"],
                route=service["route"],
                methods=" / ".join(service["methods"]),
                parameters=parameter_count,
                errors=len(service["errors"]),
                restrictions=len(service["restrictions"]),
                donation=len(service["donationFeatures"]),
            )
        )
    lines.extend(
        [
            "",
            "The machine-readable inventory preserves every documented section, table row, list item,",
            "success/error heading, restriction, and donation note for conformance-test traceability.",
            "",
        ]
    )
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--date", required=True)
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--matrix", type=Path, required=True)
    args = parser.parse_args()

    request = Request(SOURCE_URL, headers={"User-Agent": "PVLog compatibility inventory/1"})
    with urlopen(request, timeout=30) as response:
        html = response.read()
    soup = BeautifulSoup(html, "html.parser")
    inventory = {
        "schemaVersion": 1,
        "sourceUrl": SOURCE_URL,
        "retrievedAt": args.date,
        "sourceSha256": __import__("hashlib").sha256(html).hexdigest(),
        "commonContract": common_contract(soup),
        "services": extract_services(soup),
    }
    args.inventory.parent.mkdir(parents=True, exist_ok=True)
    args.matrix.parent.mkdir(parents=True, exist_ok=True)
    args.inventory.write_text(json.dumps(inventory, indent=2) + "\n", encoding="utf-8")
    args.matrix.write_text(markdown(inventory), encoding="utf-8")


if __name__ == "__main__":
    main()
