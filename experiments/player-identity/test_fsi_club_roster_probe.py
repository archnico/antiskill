import unittest
from fsi_club_roster_probe import parse_club_roster

def fake_roster(club=5004,year=2026):
    rows=[]
    for fsi,name,birth in [(100,'ROSSI MARIO',1981),(101,'VERDI ALMA',1991)]:
        row=[str(fsi),name,"",str(year),"M",str(birth),"NC","BO","EMILIA ROMAGNA",
             "ITA",str(club),"CIRCOLO DI PROVA","BO","EMI","TA 1"]
        rows.append("<tr>"+"".join("<td>"+s+"</td>" for s in row)+"</tr>")
    return ('<div>Trovati <B>2</B> giocatori.</div>'
            '<div class="table-responsive"><table><tr><th>Id</th></tr>'+
            "".join(rows)+"</table></div>").encode()

class FsiClubRosterTests(unittest.TestCase):
    def test_table_has_exact_count_and_ids(self):
        found=parse_club_roster(fake_roster(),5004,2026)
        self.assertEqual(len(found),2)
        self.assertEqual(found[100],{'name':'ROSSI MARIO','birth_year':1981})
        with self.assertRaisesRegex(ValueError,'club or year'):
            parse_club_roster(fake_roster(),5005,2026)
        with self.assertRaisesRegex(ValueError,'count'):
            parse_club_roster(fake_roster().replace(b'Trovati <B>2</B>',b'Trovati <B>3</B>'),5004,2026)

if __name__=='__main__':
    unittest.main()
